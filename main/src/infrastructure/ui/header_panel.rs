use std::convert::TryInto;

use derive_more::Display;
use std::rc::{Rc, Weak};

use rxrust::prelude::*;
use std::{iter, sync};

use enum_iterator::IntoEnumIterator;

use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};

use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId, ReaperString};
use slog::debug;

use swell_ui::{MenuBar, Pixels, Point, SharedView, View, ViewContext, Window};

use crate::application::{
    reaper_supports_global_midi_filter, Affected, CompartmentProp, ControllerPreset, FxId,
    MainPreset, MainPresetAutoLoadMode, MappingCommand, Preset, PresetManager, SessionProp,
    SharedMapping, SharedSession, VirtualControlElementType, WeakSession,
};
use crate::base::when;
use crate::domain::{
    BackboneState, ClipMatrixRef, ControlInput, FeedbackOutput, GroupId, MappingCompartment,
    MessageCaptureEvent, OscDeviceId, ParamSetting, ReaperTarget, COMPARTMENT_PARAMETER_COUNT,
};
use crate::domain::{MidiControlInput, MidiDestination};
use crate::infrastructure::data::{
    CompartmentModelData, ExtendedPresetManager, MappingModelData, OscDevice,
};
use crate::infrastructure::plugin::{
    warn_about_failed_server_start, App, RealearnPluginParameters,
};

use crate::infrastructure::ui::bindings::root;

use crate::base::notification::notify_processing_result;
use crate::infrastructure::api::convert::from_data::ConversionStyle;
use crate::infrastructure::ui::dialog_util::add_group_via_dialog;
use crate::infrastructure::ui::util::open_in_browser;
use crate::infrastructure::ui::{
    add_firewall_rule, copy_text_to_clipboard, deserialize_api_object_from_lua,
    deserialize_data_object, deserialize_data_object_from_json, get_text_from_clipboard,
    serialize_data_object, serialize_data_object_to_json, serialize_data_object_to_lua, DataObject,
    GroupFilter, GroupPanel, IndependentPanelManager, MappingRowsPanel, SearchExpression,
    SerializationFormat, SharedIndependentPanelManager, SharedMainState, SourceFilter,
};
use crate::infrastructure::ui::{dialog_util, CompanionAppPresenter};
use itertools::Itertools;
use realearn_api::schema::Envelope;
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::net::Ipv4Addr;

const OSC_INDEX_OFFSET: isize = 1000;
const KEYBOARD_INDEX_OFFSET: isize = 2000;
const PARAM_BATCH_SIZE: u32 = 5;

/// The upper part of the main panel, containing buttons such as "Add mapping".
#[derive(Debug)]
pub struct HeaderPanel {
    view: ViewContext,
    session: WeakSession,
    main_state: SharedMainState,
    companion_app_presenter: Rc<CompanionAppPresenter>,
    plugin_parameters: sync::Weak<RealearnPluginParameters>,
    panel_manager: Weak<RefCell<IndependentPanelManager>>,
    group_panel: RefCell<Option<SharedView<GroupPanel>>>,
    is_invoked_programmatically: Cell<bool>,
}

impl HeaderPanel {
    pub fn new(
        session: WeakSession,
        main_state: SharedMainState,
        plugin_parameters: sync::Weak<RealearnPluginParameters>,
        panel_manager: Weak<RefCell<IndependentPanelManager>>,
    ) -> HeaderPanel {
        HeaderPanel {
            view: Default::default(),
            session: session.clone(),
            main_state,
            companion_app_presenter: CompanionAppPresenter::new(session),
            plugin_parameters,
            panel_manager,
            group_panel: Default::default(),
            is_invoked_programmatically: false.into(),
        }
    }

    pub fn handle_affected(&self, affected: &Affected<SessionProp>, initiator: Option<u32>) {
        if !self.is_open() {
            return;
        }
        use Affected::*;
        use CompartmentProp::*;
        use SessionProp::*;
        match affected {
            One(InCompartment(compartment, One(InGroup(_, _))))
                if *compartment == self.active_compartment() =>
            {
                self.invalidate_group_controls();
            }
            _ => {}
        }
        if let Some(open_group_panel) = self.group_panel.borrow_mut().as_ref() {
            open_group_panel.handle_affected(affected, initiator);
        }
    }

    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    /// If you know a function in this view can be invoked by something else than the dialog
    /// process, wrap your function body with this. Basically all pub functions!
    ///
    /// This prevents edit control text change events fired by windows to be processed.
    fn invoke_programmatically(&self, f: impl FnOnce()) {
        self.is_invoked_programmatically.set(true);
        scopeguard::defer! { self.is_invoked_programmatically.set(false); }
        f();
    }

    fn active_compartment(&self) -> MappingCompartment {
        self.main_state.borrow().active_compartment.get()
    }

    fn active_group_id(&self) -> Option<GroupId> {
        let group_filter = self
            .main_state
            .borrow()
            .displayed_group_for_active_compartment()?;
        Some(group_filter.group_id())
    }

    fn panel_manager(&self) -> SharedIndependentPanelManager {
        self.panel_manager.upgrade().expect("panel manager gone")
    }

    fn toggle_learn_many_mappings(&self) {
        self.main_state.borrow_mut().stop_filter_learning();
        let session = self.session();
        if session.borrow().is_learning_many_mappings() {
            session.borrow_mut().stop_learning_many_mappings();
            self.panel_manager().borrow().close_message_panel();
        } else {
            let compartment = self.active_compartment();
            let control_element_type = match compartment {
                MappingCompartment::ControllerMappings => {
                    match self.prompt_for_control_element_type() {
                        None => return,
                        Some(t) => t,
                    }
                }
                MappingCompartment::MainMappings => {
                    // Doesn't matter
                    VirtualControlElementType::Multi
                }
            };
            session.borrow_mut().start_learning_many_mappings(
                &session,
                compartment,
                self.active_group_id().unwrap_or_default(),
                control_element_type,
            );
            self.panel_manager().borrow().open_message_panel();
        }
    }

    fn prompt_for_control_element_type(&self) -> Option<VirtualControlElementType> {
        let menu_bar = MenuBar::load(root::IDR_HEADER_PANEL_ADD_MANY_CONTROLLER_MAPPINGS_MENU)
            .expect("menu bar couldn't be loaded");
        let menu = menu_bar
            .get_sub_menu(0)
            .expect("menu bar didn't have 1st menu");
        let location = Window::cursor_pos();
        let result = self.view.require_window().open_popup_menu(menu, location)?;
        let control_element_type = match result {
            root::IDM_MULTIS => VirtualControlElementType::Multi,
            root::IDM_BUTTONS => VirtualControlElementType::Button,
            _ => unreachable!(),
        };
        Some(control_element_type)
    }

    fn add_group(&self) {
        if let Ok(group_id) = self.add_group_internal() {
            self.main_state
                .borrow_mut()
                .set_displayed_group_for_active_compartment(Some(GroupFilter(group_id)));
        }
    }

    fn add_group_internal(&self) -> Result<GroupId, &'static str> {
        add_group_via_dialog(self.session(), self.active_compartment())
    }

    fn add_mapping(&self) {
        self.main_state.borrow_mut().clear_all_filters();
        self.session().borrow_mut().add_default_mapping(
            self.active_compartment(),
            self.active_group_id().unwrap_or_default(),
            VirtualControlElementType::Multi,
        );
    }

    fn open_context_menu(&self, location: Point<Pixels>) -> Result<(), &'static str> {
        let app = App::get();
        let menu_bar = MenuBar::new_popup_menu();
        enum MenuAction {
            None,
            CopyListedMappingsAsJson,
            CopyListedMappingsAsLua(ConversionStyle),
            AutoNameListedMappings,
            MakeTargetsOfListedMappingsSticky,
            MakeSourcesOfMainMappingsVirtual,
            MoveListedMappingsToGroup(Option<GroupId>),
            PasteReplaceAllInGroup(Vec<MappingModelData>),
            PasteFromLuaReplaceAllInGroup(String),
            ToggleAutoCorrectSettings,
            ToggleRealInputLogging,
            ToggleVirtualInputLogging,
            ToggleRealOutputLogging,
            ToggleVirtualOutputLogging,
            ToggleSendFeedbackOnlyIfTrackArmed,
            ToggleUpperFloorMembership,
            ToggleServer,
            AddFirewallRule,
            ChangeSessionId,
            EditPresetLinkFxId(FxId),
            RemovePresetLink(FxId),
            LinkToPreset(FxId, String),
            OpenOfflineUserGuide,
            OpenOnlineUserGuide,
            OpenForum,
            ContactDeveloper,
            OpenWebsite,
            Donate,
            ReloadAllPresets,
            EditNewOscDevice,
            EditExistingOscDevice(OscDeviceId),
            RemoveOscDevice(OscDeviceId),
            ToggleOscDeviceControl(OscDeviceId),
            ToggleOscDeviceFeedback(OscDeviceId),
            ToggleOscDeviceBundles(OscDeviceId),
            EditCompartmentParameter(MappingCompartment, u32),
            SendFeedbackNow,
            LogDebugInfo,
        }
        impl Default for MenuAction {
            fn default() -> Self {
                Self::None
            }
        }
        let pure_menu = {
            use std::iter::once;
            use swell_ui::menu_tree::*;
            let dev_manager = App::get().osc_device_manager();
            let dev_manager = dev_manager.borrow();
            let preset_link_manager = App::get().preset_link_manager();
            let preset_link_manager = preset_link_manager.borrow();
            let main_preset_manager = App::get().main_preset_manager();
            let main_preset_manager = main_preset_manager.borrow();
            let text_from_clipboard = get_text_from_clipboard();
            let data_object_from_clipboard = text_from_clipboard
                .as_ref()
                .and_then(|text| deserialize_data_object_from_json(text).ok());
            let clipboard_could_contain_lua =
                text_from_clipboard.is_some() && data_object_from_clipboard.is_none();
            let session = self.session();
            let session = session.borrow();
            let session_state = session.state().borrow();
            let compartment = self.active_compartment();
            let group_id = self.active_group_id();
            let last_focused_fx_id = App::get().previously_focused_fx().and_then(|fx| {
                if fx.is_available() {
                    FxId::from_fx(&fx, true).ok()
                } else {
                    None
                }
            });
            let entries = vec![
                item("Copy listed mappings", || {
                    MenuAction::CopyListedMappingsAsJson
                }),
                {
                    if let Some(DataObject::Mappings(env)) = data_object_from_clipboard {
                        item(
                            format!("Paste {} mappings (replace all in group)", env.value.len()),
                            move || MenuAction::PasteReplaceAllInGroup(env.value),
                        )
                    } else {
                        disabled_item("Paste mappings (replace all in group)")
                    }
                },
                item("Auto-name listed mappings", || {
                    MenuAction::AutoNameListedMappings
                }),
                item("Make sources of all main mappings virtual", || {
                    MenuAction::MakeSourcesOfMainMappingsVirtual
                }),
                item("Make targets of listed mappings sticky", || {
                    MenuAction::MakeTargetsOfListedMappingsSticky
                }),
                menu(
                    "Move listed mappings to group",
                    iter::once(item("<New group>", || {
                        MenuAction::MoveListedMappingsToGroup(None)
                    }))
                    .chain(session.groups_sorted(compartment).map(move |g| {
                        let g = g.borrow();
                        let g_id = g.id();
                        item_with_opts(
                            g.to_string(),
                            ItemOpts {
                                enabled: group_id != Some(g_id),
                                checked: false,
                            },
                            move || MenuAction::MoveListedMappingsToGroup(Some(g_id)),
                        )
                    }))
                    .collect(),
                ),
                menu(
                    "Options",
                    vec![
                        item_with_opts(
                            "Auto-correct settings",
                            ItemOpts {
                                enabled: true,
                                checked: session.auto_correct_settings.get(),
                            },
                            || MenuAction::ToggleAutoCorrectSettings,
                        ),
                        item_with_opts(
                            "Send feedback only if track armed",
                            if session.containing_fx_is_in_input_fx_chain() {
                                ItemOpts {
                                    enabled: false,
                                    checked: true,
                                }
                            } else {
                                ItemOpts {
                                    enabled: true,
                                    checked: session.send_feedback_only_if_armed.get(),
                                }
                            },
                            || MenuAction::ToggleSendFeedbackOnlyIfTrackArmed,
                        ),
                        item_with_opts(
                            "Make instance superior",
                            ItemOpts {
                                enabled: true,
                                checked: session.lives_on_upper_floor.get(),
                            },
                            || MenuAction::ToggleUpperFloorMembership,
                        ),
                    ],
                ),
                menu(
                    "Compartment parameters",
                    (0..COMPARTMENT_PARAMETER_COUNT / PARAM_BATCH_SIZE)
                        .map(|batch_index| {
                            let offset = batch_index * PARAM_BATCH_SIZE;
                            let range = offset..(offset + PARAM_BATCH_SIZE);
                            menu(
                                format!("Parameters {} - {}", range.start + 1, range.end),
                                range
                                    .map(|i| {
                                        item(
                                            format!(
                                                "{}...",
                                                session_state.get_parameter_name(compartment, i)
                                            ),
                                            move || {
                                                MenuAction::EditCompartmentParameter(compartment, i)
                                            },
                                        )
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                ),
                menu(
                    "Advanced",
                    vec![
                        item("Copy listed mappings as Lua", || {
                            MenuAction::CopyListedMappingsAsLua(ConversionStyle::Minimal)
                        }),
                        item(
                            "Copy listed mappings as Lua (include default values)",
                            || {
                                MenuAction::CopyListedMappingsAsLua(
                                    ConversionStyle::IncludeDefaultValues,
                                )
                            },
                        ),
                        item_with_opts(
                            "Paste from Lua (replace all in group)",
                            ItemOpts {
                                enabled: clipboard_could_contain_lua,
                                checked: false,
                            },
                            move || {
                                MenuAction::PasteFromLuaReplaceAllInGroup(
                                    text_from_clipboard.unwrap(),
                                )
                            },
                        ),
                    ],
                ),
                separator(),
                menu(
                    "Server",
                    vec![
                        item_with_opts(
                            "Enabled",
                            ItemOpts {
                                enabled: true,
                                checked: App::get().config().server_is_enabled(),
                            },
                            || MenuAction::ToggleServer,
                        ),
                        item("Add firewall rule", || MenuAction::AddFirewallRule),
                        item("Change session ID...", || MenuAction::ChangeSessionId),
                    ],
                ),
                menu(
                    "OSC devices",
                    once(item("<New>", || MenuAction::EditNewOscDevice))
                        .chain(dev_manager.devices().map(|dev| {
                            let dev_id = *dev.id();
                            menu(
                                dev.name(),
                                vec![
                                    item("Edit...", move || {
                                        MenuAction::EditExistingOscDevice(dev_id)
                                    }),
                                    item("Remove", move || MenuAction::RemoveOscDevice(dev_id)),
                                    item_with_opts(
                                        "Enabled for control",
                                        ItemOpts {
                                            enabled: true,
                                            checked: dev.is_enabled_for_control(),
                                        },
                                        move || MenuAction::ToggleOscDeviceControl(dev_id),
                                    ),
                                    item_with_opts(
                                        "Enabled for feedback",
                                        ItemOpts {
                                            enabled: true,
                                            checked: dev.is_enabled_for_feedback(),
                                        },
                                        move || MenuAction::ToggleOscDeviceFeedback(dev_id),
                                    ),
                                    item_with_opts(
                                        "Can deal with OSC bundles",
                                        ItemOpts {
                                            enabled: true,
                                            checked: dev.can_deal_with_bundles(),
                                        },
                                        move || MenuAction::ToggleOscDeviceBundles(dev_id),
                                    ),
                                ],
                            )
                        }))
                        .collect(),
                ),
                menu(
                    "FX-to-preset links",
                    once(if let Some(fx_id) = last_focused_fx_id {
                        menu(
                            format!("<Add link from FX \"{}\" to ...>", fx_id),
                            main_preset_manager
                                .preset_iter()
                                .map(move |p| {
                                    let fx_id = fx_id.clone();
                                    let preset_id = p.id().to_owned();
                                    item(p.name(), move || {
                                        MenuAction::LinkToPreset(fx_id, preset_id)
                                    })
                                })
                                .collect(),
                        )
                    } else {
                        disabled_item("<Add link from last focused FX to preset>")
                    })
                    .chain(preset_link_manager.links().map(|link| {
                        let fx_id_0 = link.fx_id.clone();
                        let fx_id_1 = link.fx_id.clone();
                        let fx_id_2 = link.fx_id.clone();
                        let preset_id_0 = link.preset_id.clone();
                        menu(
                            link.fx_id.to_string(),
                            once(item("<Edit FX ID...>", move || {
                                MenuAction::EditPresetLinkFxId(fx_id_0)
                            }))
                            .chain(once(item("<Remove link>", move || {
                                MenuAction::RemovePresetLink(fx_id_1)
                            })))
                            .chain(main_preset_manager.preset_iter().map(move |p| {
                                let fx_id = fx_id_2.clone();
                                let preset_id = p.id().to_owned();
                                item_with_opts(
                                    p.name(),
                                    ItemOpts {
                                        enabled: true,
                                        checked: p.id() == preset_id_0,
                                    },
                                    move || MenuAction::LinkToPreset(fx_id, preset_id),
                                )
                            }))
                            .chain(once(
                                if main_preset_manager
                                    .find_index_by_id(&link.preset_id)
                                    .is_some()
                                {
                                    Entry::Nothing
                                } else {
                                    disabled_item(format!("<Not present> ({})", link.preset_id))
                                },
                            ))
                            .collect(),
                        )
                    }))
                    .collect(),
                ),
                menu(
                    "Help",
                    vec![
                        item("User guide for this version (PDF, offline)", || {
                            MenuAction::OpenOfflineUserGuide
                        }),
                        item("User guide for latest version (HTML, online)", || {
                            MenuAction::OpenOnlineUserGuide
                        }),
                        item("Forum", || MenuAction::OpenForum),
                        item("Contact developer", || MenuAction::ContactDeveloper),
                        item("Website", || MenuAction::OpenWebsite),
                        item("Donate", || MenuAction::Donate),
                    ],
                ),
                item("Reload all presets from disk", || {
                    MenuAction::ReloadAllPresets
                }),
                separator(),
                item("Send feedback now", || MenuAction::SendFeedbackNow),
                item("Log debug info", || MenuAction::LogDebugInfo),
                item_with_opts(
                    "Log incoming real messages",
                    ItemOpts {
                        enabled: true,
                        checked: session.real_input_logging_enabled.get(),
                    },
                    || MenuAction::ToggleRealInputLogging,
                ),
                item_with_opts(
                    "Log incoming virtual messages",
                    ItemOpts {
                        enabled: true,
                        checked: session.virtual_input_logging_enabled.get(),
                    },
                    || MenuAction::ToggleVirtualInputLogging,
                ),
                item_with_opts(
                    "Log outgoing real messages",
                    ItemOpts {
                        enabled: true,
                        checked: session.real_output_logging_enabled.get(),
                    },
                    || MenuAction::ToggleRealOutputLogging,
                ),
                item_with_opts(
                    "Log outgoing virtual messages",
                    ItemOpts {
                        enabled: true,
                        checked: session.virtual_output_logging_enabled.get(),
                    },
                    || MenuAction::ToggleVirtualOutputLogging,
                ),
            ];
            let mut root_menu = root_menu(entries);
            root_menu.index(1);
            fill_menu(menu_bar.menu(), &root_menu);
            root_menu
        };
        // Open menu
        let result_index = self
            .view
            .require_window()
            .open_popup_menu(menu_bar.menu(), location)
            .ok_or("no entry selected")?;
        let result = pure_menu
            .find_item_by_id(result_index)
            .expect("selected menu item not found")
            .invoke_handler();
        // Execute action
        match result {
            MenuAction::None => {}
            MenuAction::CopyListedMappingsAsJson => {
                self.copy_listed_mappings_as_json().unwrap();
            }
            MenuAction::AutoNameListedMappings => self.auto_name_listed_mappings(),
            MenuAction::MakeSourcesOfMainMappingsVirtual => {
                self.make_sources_of_main_mappings_virtual()
            }
            MenuAction::MakeTargetsOfListedMappingsSticky => {
                self.make_targets_of_listed_mappings_sticky()
            }
            MenuAction::MoveListedMappingsToGroup(group_id) => {
                let _ = self.move_listed_mappings_to_group(group_id);
            }
            MenuAction::PasteReplaceAllInGroup(mapping_datas) => {
                self.paste_replace_all_in_group(mapping_datas)
            }
            MenuAction::CopyListedMappingsAsLua(style) => {
                self.copy_listed_mappings_as_lua(style).unwrap()
            }
            MenuAction::PasteFromLuaReplaceAllInGroup(text) => {
                self.paste_from_lua_replace_all_in_group(&text);
            }
            MenuAction::EditNewOscDevice => edit_new_osc_device(),
            MenuAction::EditExistingOscDevice(dev_id) => edit_existing_osc_device(dev_id),
            MenuAction::RemoveOscDevice(dev_id) => {
                remove_osc_device(self.view.require_window(), dev_id)
            }
            MenuAction::ToggleOscDeviceControl(dev_id) => {
                App::get().do_with_osc_device(dev_id, |d| d.toggle_control())
            }
            MenuAction::ToggleOscDeviceFeedback(dev_id) => {
                App::get().do_with_osc_device(dev_id, |d| d.toggle_feedback())
            }
            MenuAction::ToggleOscDeviceBundles(dev_id) => {
                App::get().do_with_osc_device(dev_id, |d| d.toggle_can_deal_with_bundles())
            }
            MenuAction::EditCompartmentParameter(compartment, rel_index) => {
                let _ = edit_compartment_parameter(self.session(), compartment, rel_index);
            }
            MenuAction::ToggleAutoCorrectSettings => self.toggle_always_auto_detect(),
            MenuAction::ToggleRealInputLogging => self.toggle_real_input_logging(),
            MenuAction::ToggleVirtualInputLogging => self.toggle_virtual_input_logging(),
            MenuAction::ToggleRealOutputLogging => self.toggle_real_output_logging(),
            MenuAction::ToggleVirtualOutputLogging => self.toggle_virtual_output_logging(),
            MenuAction::ToggleSendFeedbackOnlyIfTrackArmed => {
                self.toggle_send_feedback_only_if_armed()
            }
            MenuAction::ToggleUpperFloorMembership => self.toggle_upper_floor_membership(),
            MenuAction::ToggleServer => {
                enum ServerAction {
                    Start,
                    Disable,
                    Enable,
                }
                let next_server_action = {
                    let server = app.server().borrow();
                    let next_server_action = {
                        use ServerAction::*;
                        if server.is_running() {
                            if app.config().server_is_enabled() {
                                Disable
                            } else {
                                Enable
                            }
                        } else {
                            Start
                        }
                    };
                    next_server_action
                };
                match next_server_action {
                    ServerAction::Start => {
                        match App::start_server_persistently(app) {
                            Ok(_) => {
                                self.view
                                    .require_window()
                                    .alert("ReaLearn", "Successfully started projection server.");
                            }
                            Err(info) => {
                                warn_about_failed_server_start(info);
                            }
                        };
                    }
                    ServerAction::Disable => {
                        app.disable_server_persistently();
                        self.view.require_window().alert(
                                    "ReaLearn",
                                    "Disabled projection server. This will take effect on the next start of REAPER.",
                                );
                    }
                    ServerAction::Enable => {
                        app.enable_server_persistently();
                        self.view
                            .require_window()
                            .alert("ReaLearn", "Enabled projection server again.");
                    }
                }
            }
            MenuAction::AddFirewallRule => {
                let (http_port, https_port) = {
                    let server = app.server().borrow();
                    (server.http_port(), server.https_port())
                };
                let msg = match add_firewall_rule(http_port, https_port) {
                    Ok(_) => "Successfully added firewall rule.".to_string(),
                    Err(reason) => format!(
                        "Couldn't add firewall rule because {}. Please try to do it manually!",
                        reason
                    ),
                };
                self.view.require_window().alert("ReaLearn", msg);
            }
            MenuAction::ChangeSessionId => self.change_session_id(),
            MenuAction::OpenOfflineUserGuide => self.open_user_guide_offline(),
            MenuAction::OpenOnlineUserGuide => self.open_user_guide_online(),
            MenuAction::OpenForum => self.open_forum(),
            MenuAction::ContactDeveloper => self.contact_developer(),
            MenuAction::OpenWebsite => self.open_website(),
            MenuAction::Donate => self.donate(),
            MenuAction::ReloadAllPresets => self.reload_all_presets(),
            MenuAction::SendFeedbackNow => self.session().borrow().send_all_feedback(),
            MenuAction::LogDebugInfo => self.log_debug_info(),
            MenuAction::EditPresetLinkFxId(fx_id) => edit_preset_link_fx_id(fx_id),
            MenuAction::RemovePresetLink(fx_id) => remove_preset_link(fx_id),
            MenuAction::LinkToPreset(fx_id, preset_id) => link_to_preset(fx_id, preset_id),
        };
        Ok(())
    }

    fn copy_listed_mappings_as_json(&self) -> Result<(), Box<dyn Error>> {
        let data_object = self.get_listened_mappings_as_data_object();
        let json = serialize_data_object_to_json(data_object)?;
        copy_text_to_clipboard(json);
        Ok(())
    }

    fn copy_listed_mappings_as_lua(
        &self,
        conversion_style: ConversionStyle,
    ) -> Result<(), Box<dyn Error>> {
        let data_object = self.get_listened_mappings_as_data_object();
        let json = serialize_data_object_to_lua(data_object, conversion_style)?;
        copy_text_to_clipboard(json);
        Ok(())
    }

    fn get_listened_mappings_as_data_object(&self) -> DataObject {
        let session = self.session();
        let session = session.borrow();
        let compartment = self.active_compartment();
        let compartment_in_session = session.compartment_in_session(compartment);
        let mapping_datas = self
            .get_listened_mappings(compartment)
            .iter()
            .map(|m| MappingModelData::from_model(&*m.borrow(), &compartment_in_session))
            .collect();
        DataObject::Mappings(Envelope {
            value: mapping_datas,
        })
    }

    fn auto_name_listed_mappings(&self) {
        let listed_mappings = self.get_listened_mappings(self.active_compartment());
        if listed_mappings.is_empty() {
            return;
        }
        if !self.view.require_window().confirm(
            "ReaLearn",
            format!(
                "This clears the names of {} mappings, which in turn makes them use the auto-generated name. Do you really want to continue?",
                listed_mappings.len()
            ),
        ) {
            return;
        }
        let session = self.session();
        let mut session = session.borrow_mut();
        for m in listed_mappings {
            let mut mapping = m.borrow_mut();
            session.change_mapping_from_ui_expert(
                &mut mapping,
                MappingCommand::ClearName,
                None,
                self.session.clone(),
            );
        }
    }

    fn make_sources_of_main_mappings_virtual(&self) {
        if !self.view.require_window().confirm(
            "ReaLearn",
            "This will attempt to make the sources in the main compartment virtual by matching them with the sources in the controller compartment. Do you really want to continue?",
        ) {
            return;
        }
        let shared_session = self.session();
        let result = {
            let mut session = shared_session.borrow_mut();
            session.virtualize_main_mappings()
        };
        self.notify_user_on_error(result.map_err(|e| e.into()));
    }

    fn make_targets_of_listed_mappings_sticky(&self) {
        let compartment = self.active_compartment();
        let listed_mappings = self.get_listened_mappings(compartment);
        if listed_mappings.is_empty() {
            return;
        }
        if !self.view.require_window().confirm(
            "ReaLearn",
            format!(
                "This will change the targets of {} mappings to use sticky track/FX/send selectors such as <Master>, <This> and By ID. Do you really want to continue?",
                listed_mappings.len()
            ),
        ) {
            return;
        }
        let session = self.session();
        let mut session = session.borrow_mut();
        let context = session.extended_context();
        let errors: Vec<_> = listed_mappings
            .iter()
            .filter_map(|m| {
                let mut m = m.borrow_mut();
                m.make_target_sticky(context).err().map(|e| {
                    format!(
                        "Couldn't make target of mapping {} sticky because {}",
                        m.effective_name(),
                        e
                    )
                })
            })
            .collect();
        session.notify_compartment_has_changed(compartment, self.session.clone());
        if !errors.is_empty() {
            notify_processing_result("Errors occurred when making targets sticky", errors);
        }
    }

    fn move_listed_mappings_to_group(&self, group_id: Option<GroupId>) -> Result<(), &'static str> {
        let group_id = group_id
            .or_else(|| self.add_group_internal().ok())
            .ok_or("no group selected")?;
        let compartment = self.active_compartment();
        let listed_mappings = self.get_listened_mappings(compartment);
        if listed_mappings.is_empty() {
            return Err("mapping list empty");
        }
        if !self.view.require_window().confirm(
            "ReaLearn",
            format!(
                "Do you really want to move {} mappings to the specified group?",
                listed_mappings.len()
            ),
        ) {
            return Err("cancelled");
        }
        let session = self.session();
        let mut session = session.borrow_mut();
        let mapping_ids: Vec<_> = listed_mappings
            .into_iter()
            .map(|m| m.borrow().id())
            .collect();
        session.move_mappings_to_group(
            compartment,
            &mapping_ids,
            group_id,
            self.session.clone(),
        )?;
        Ok(())
    }

    fn get_listened_mappings(&self, compartment: MappingCompartment) -> Vec<SharedMapping> {
        let main_state = self.main_state.borrow();
        let session = self.session();
        let session = session.borrow();
        MappingRowsPanel::filtered_mappings(&session, &main_state, compartment, false)
            .cloned()
            .collect()
    }

    fn paste_from_lua_replace_all_in_group(&self, text: &str) {
        if let Err(e) = self.paste_from_lua_replace_all_in_group_internal(text) {
            self.view.require_window().alert("ReaLearn", e.to_string());
        }
    }

    fn paste_from_lua_replace_all_in_group_internal(
        &self,
        text: &str,
    ) -> Result<(), Box<dyn Error>> {
        let api_object = deserialize_api_object_from_lua(text)?;
        let api_mappings = api_object
            .into_mappings()
            .ok_or("Can only paste a list of mappings into a mapping group.")?;
        let data_mappings = {
            let session = self.session();
            let session = session.borrow();
            let compartment_in_session = session.compartment_in_session(self.active_compartment());
            DataObject::try_from_api_mappings(api_mappings, &compartment_in_session)?
        };
        self.paste_replace_all_in_group(data_mappings);
        Ok(())
    }

    // https://github.com/rust-lang/rust-clippy/issues/6066
    #[allow(clippy::needless_collect)]
    fn paste_replace_all_in_group(&self, mapping_datas: Vec<MappingModelData>) {
        let main_state = self.main_state.borrow();
        let group_id = main_state
            .displayed_group_for_active_compartment()
            .map(|f| f.group_id())
            .unwrap_or_default();
        let compartment = main_state.active_compartment.get();
        let session = self.session();
        let mut session = session.borrow_mut();
        let group_key = if let Some(group) = session.find_group_by_id(compartment, group_id) {
            group.borrow().key().clone()
        } else {
            return;
        };
        let mapping_models: Vec<_> = mapping_datas
            .into_iter()
            .map(|mut data| {
                data.group_id = group_key.clone();
                data.to_model(
                    compartment,
                    session.compartment_in_session(compartment),
                    Some(session.extended_context()),
                )
            })
            .collect();
        session.replace_mappings_of_group(compartment, group_id, mapping_models.into_iter());
    }

    fn toggle_learn_source_filter(&self) {
        let mut main_state = self.main_state.borrow_mut();
        let active_compartment = main_state.active_compartment.get();
        let learning = &mut main_state.is_learning_source_filter;
        if learning.get() {
            // Stop learning
            learning.set(false);
        } else {
            // Start learning
            learning.set(true);
            let main_state_1 = self.main_state.clone();
            let main_state_2 = self.main_state.clone();
            when(
                self.session()
                    .borrow()
                    .incoming_msg_captured(
                        true,
                        active_compartment != MappingCompartment::ControllerMappings,
                        None,
                    )
                    .take_until(learning.changed_to(false))
                    .take_until(self.view.closed()),
            )
            .with(self.session.clone())
            .finally(move |_| {
                main_state_1
                    .borrow_mut()
                    .is_learning_source_filter
                    .set(false);
            })
            .do_async(move |session, capture_event: MessageCaptureEvent| {
                let virtual_source_value = if capture_event.allow_virtual_sources {
                    session
                        .borrow()
                        .virtualize_source_value(capture_event.result.message())
                } else {
                    None
                };
                let filter = SourceFilter {
                    message_capture_result: capture_event.result,
                    virtual_source_value,
                };
                main_state_2.borrow_mut().source_filter.set(Some(filter));
            });
        }
    }

    fn toggle_learn_target_filter(&self) {
        let mut main_state = self.main_state.borrow_mut();
        let learning = &mut main_state.is_learning_target_filter;
        if learning.get() {
            // Stop learning
            learning.set(false);
        } else {
            // Start learning
            learning.set(true);
            when(
                ReaperTarget::touched()
                    .take_until(learning.changed_to(false))
                    .take_until(self.view.closed())
                    .take(1),
            )
            .with(Rc::downgrade(&self.main_state))
            .finally(|main_state| {
                main_state.borrow_mut().is_learning_target_filter.set(false);
            })
            .do_sync(|main_state, target| {
                main_state
                    .borrow_mut()
                    .target_filter
                    .set(Some((*target).clone()));
            });
        }
    }

    fn clear_source_filter(&self) {
        let mut main_state = self.main_state.borrow_mut();
        main_state.clear_source_filter();
        // When clearing the source filter while still learning, we want the learning to stop, too.
        let learning = &mut main_state.is_learning_source_filter;
        if learning.get() {
            // Stop learning
            learning.set(false);
        }
    }

    fn clear_target_filter(&self) {
        self.main_state.borrow_mut().clear_target_filter();
    }

    fn clear_search_expression(&self) {
        self.main_state
            .borrow_mut()
            .clear_search_expression_filter();
    }

    fn update_let_matched_events_through(&self) {
        self.session().borrow_mut().let_matched_events_through.set(
            self.view
                .require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_let_unmatched_events_through(&self) {
        self.session()
            .borrow_mut()
            .let_unmatched_events_through
            .set(
                self.view
                    .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX)
                    .is_checked(),
            );
    }

    fn toggle_send_feedback_only_if_armed(&self) {
        self.session()
            .borrow_mut()
            .send_feedback_only_if_armed
            .set_with(|prev| !*prev);
    }

    fn toggle_always_auto_detect(&self) {
        self.session()
            .borrow_mut()
            .auto_correct_settings
            .set_with(|prev| !*prev);
    }

    fn toggle_real_input_logging(&self) {
        self.session()
            .borrow_mut()
            .real_input_logging_enabled
            .set_with(|prev| !*prev);
    }

    fn toggle_virtual_input_logging(&self) {
        self.session()
            .borrow_mut()
            .virtual_input_logging_enabled
            .set_with(|prev| !*prev);
    }

    fn toggle_real_output_logging(&self) {
        self.session()
            .borrow_mut()
            .real_output_logging_enabled
            .set_with(|prev| !*prev);
    }

    fn toggle_virtual_output_logging(&self) {
        self.session()
            .borrow_mut()
            .virtual_output_logging_enabled
            .set_with(|prev| !*prev);
    }

    fn toggle_upper_floor_membership(&self) {
        let enabled = {
            let session = self.session();
            let mut session = session.borrow_mut();
            let new_state = !session.lives_on_upper_floor.get();
            session.lives_on_upper_floor.set(new_state);
            new_state
        };
        if enabled {
            let msg = "This ReaLearn instance is now superior. When this instance is active (contains active main mappings), it will disable other ReaLearn instances with the same control input and/or feedback output that don't have this setting turned on.";
            self.view.require_window().alert("ReaLearn", msg);
        };
    }

    fn fill_all_controls(&self) {
        self.fill_preset_auto_load_mode_combo_box();
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_control_input_combo_box();
        self.invalidate_feedback_output_combo_box();
        self.invalidate_compartment_combo_box();
        self.invalidate_preset_controls();
        self.invalidate_group_controls();
        self.invalidate_let_through_controls();
        self.invalidate_source_filter_buttons();
        self.invalidate_target_filter_buttons();
        self.invalidate_add_one_button();
        self.invalidate_learn_many_button();
    }

    fn invalidate_let_through_controls(&self) {
        let label = self.view.require_control(root::ID_LET_THROUGH_LABEL_TEXT);
        let matched_box = self
            .view
            .require_control(root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX);
        let unmatched_box = self
            .view
            .require_control(root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX);
        let session = self.session();
        let session = session.borrow();
        let controls = [label, matched_box, unmatched_box];
        let (visible, enabled) = match session.control_input() {
            ControlInput::Midi(input) => (
                true,
                input == MidiControlInput::FxInput || reaper_supports_global_midi_filter(),
            ),
            ControlInput::Osc(_) => (false, false),
            ControlInput::Keyboard => (true, true),
        };
        for c in controls {
            c.set_visible(visible);
        }
        if visible {
            for c in controls {
                c.set_enabled(enabled);
            }
            matched_box.set_checked(session.let_matched_events_through.get());
            unmatched_box.set_checked(session.let_unmatched_events_through.get());
        }
    }

    fn invalidate_control_input_combo_box(&self) {
        self.invalidate_control_input_combo_box_options();
        self.invalidate_control_input_combo_box_value();
    }

    fn invalidate_compartment_combo_box(&self) {
        let controller_radio = self
            .view
            .require_control(root::ID_CONTROLLER_COMPARTMENT_RADIO_BUTTON);
        let main_radio = self
            .view
            .require_control(root::ID_MAIN_COMPARTMENT_RADIO_BUTTON);
        match self.active_compartment() {
            MappingCompartment::ControllerMappings => {
                controller_radio.check();
                main_radio.uncheck();
            }
            MappingCompartment::MainMappings => {
                controller_radio.uncheck();
                main_radio.check()
            }
        };
    }

    fn invalidate_preset_auto_load_mode_combo_box(&self) {
        let label = self.view.require_control(root::ID_AUTO_LOAD_LABEL_TEXT);
        let combo = self.view.require_control(root::ID_AUTO_LOAD_COMBO_BOX);
        if self.active_compartment() == MappingCompartment::MainMappings {
            label.show();
            combo.show();
            combo
                .select_combo_box_item_by_index(
                    self.session()
                        .borrow()
                        .main_preset_auto_load_mode
                        .get()
                        .into(),
                )
                .unwrap();
        } else {
            label.hide();
            combo.hide();
        }
    }

    fn invalidate_group_controls(&self) {
        self.invalidate_group_combo_box();
        self.invalidate_group_buttons();
    }

    fn invalidate_group_combo_box(&self) {
        self.fill_group_combo_box();
        self.invalidate_group_combo_box_value();
    }

    fn fill_group_combo_box(&self) {
        let combo = self.view.require_control(root::ID_GROUP_COMBO_BOX);
        let vec = vec![(-1isize, "<All>".to_string())];
        let compartment = self.active_compartment();
        combo.fill_combo_box_with_data_small(
            vec.into_iter().chain(
                self.session()
                    .borrow()
                    .groups_sorted(compartment)
                    .enumerate()
                    .map(|(i, g)| (i as isize, g.borrow().to_string())),
            ),
        );
    }

    fn invalidate_group_combo_box_value(&self) {
        let combo = self.view.require_control(root::ID_GROUP_COMBO_BOX);
        let enabled = !self.mappings_are_read_only();
        let compartment = self.active_compartment();
        let data = match self
            .main_state
            .borrow()
            .displayed_group_for_active_compartment()
        {
            None => -1isize,
            Some(GroupFilter(id)) => {
                match self
                    .session()
                    .borrow()
                    .find_group_index_by_id_sorted(compartment, id)
                {
                    None => {
                        combo.select_new_combo_box_item(format!("<Not present> ({})", id));
                        return;
                    }
                    Some(i) => i as isize,
                }
            }
        };
        combo.select_combo_box_item_by_data(data).unwrap();
        combo.set_enabled(enabled);
    }

    fn invalidate_group_buttons(&self) {
        let add_button = self.view.require_control(root::ID_GROUP_ADD_BUTTON);
        let remove_button = self.view.require_control(root::ID_GROUP_DELETE_BUTTON);
        let edit_button = self.view.require_control(root::ID_GROUP_EDIT_BUTTON);
        let (add_enabled, remove_enabled, edit_enabled) = if self.mappings_are_read_only() {
            (false, false, false)
        } else {
            match self
                .main_state
                .borrow()
                .displayed_group_for_active_compartment()
            {
                None => (true, false, false),
                Some(GroupFilter(id)) if id.is_default() => (true, false, true),
                _ => (true, true, true),
            }
        };
        add_button.set_enabled(add_enabled);
        remove_button.set_enabled(remove_enabled);
        edit_button.set_enabled(edit_enabled);
    }

    fn invalidate_preset_controls(&self) {
        self.invalidate_preset_label_text();
        self.invalidate_preset_combo_box();
        self.invalidate_preset_buttons();
        self.invalidate_preset_auto_load_mode_combo_box();
    }

    fn invalidate_preset_label_text(&self) {
        let text = match self.active_compartment() {
            MappingCompartment::ControllerMappings => "Controller preset",
            MappingCompartment::MainMappings => "Main preset",
        };
        self.view
            .require_control(root::ID_PRESET_LABEL_TEXT)
            .set_text(text);
    }

    fn invalidate_preset_combo_box(&self) {
        self.fill_preset_combo_box();
        self.invalidate_preset_combo_box_value();
    }

    fn invalidate_preset_buttons(&self) {
        let save_button = self.view.require_control(root::ID_PRESET_SAVE_BUTTON);
        let save_as_button = self.view.require_control(root::ID_PRESET_SAVE_AS_BUTTON);
        let delete_button = self.view.require_control(root::ID_PRESET_DELETE_BUTTON);
        let (save_button_enabled, save_as_button_enabled, delete_button_enabled) = {
            if self.mappings_are_read_only() {
                (false, false, false)
            } else {
                let session = self.session();
                let session = session.borrow();
                let compartment = self.active_compartment();
                let preset_is_active_and_exists =
                    if let Some(preset_id) = session.active_preset_id(compartment) {
                        App::get().preset_manager(compartment).exists(preset_id)
                    } else {
                        false
                    };
                let preset_is_dirty = session.compartment_or_preset_is_dirty(compartment);
                (
                    preset_is_active_and_exists && preset_is_dirty,
                    true,
                    preset_is_active_and_exists,
                )
            }
        };
        save_button.set_enabled(save_button_enabled);
        save_as_button.set_enabled(save_as_button_enabled);
        delete_button.set_enabled(delete_button_enabled);
    }

    fn fill_preset_combo_box(&self) {
        let combo = self.view.require_control(root::ID_PRESET_COMBO_BOX);
        let preset_manager = App::get().preset_manager(self.active_compartment());
        let all_entries = [(-1isize, "<None>".to_string())].into_iter().chain(
            preset_manager
                .preset_infos()
                .into_iter()
                .enumerate()
                .map(|(i, info)| (i as isize, info.name)),
        );
        combo.fill_combo_box_with_data_small(all_entries);
    }

    fn invalidate_preset_combo_box_value(&self) {
        let combo = self.view.require_control(root::ID_PRESET_COMBO_BOX);
        let enabled = !self.mappings_are_read_only();
        let session = self.session();
        let session = session.borrow();
        let compartment = self.active_compartment();
        let preset_manager = App::get().preset_manager(compartment);
        let data = match session.active_preset_id(compartment) {
            None => -1isize,
            Some(id) => match preset_manager.find_index_by_id(id) {
                None => {
                    combo.select_new_combo_box_item(format!("<Not present> ({})", id));
                    return;
                }
                Some(i) => i as isize,
            },
        };
        combo.select_combo_box_item_by_data(data).unwrap();
        combo.set_enabled(enabled);
    }

    fn fill_preset_auto_load_mode_combo_box(&self) {
        self.view
            .require_control(root::ID_AUTO_LOAD_COMBO_BOX)
            .fill_combo_box_indexed(MainPresetAutoLoadMode::into_enum_iter());
    }

    fn invalidate_control_input_combo_box_options(&self) {
        let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
        let osc_device_manager = App::get().osc_device_manager();
        let osc_device_manager = osc_device_manager.borrow();
        let osc_devices = osc_device_manager.devices();
        b.fill_combo_box_with_data_small(
            [
                (-100isize, generate_midi_device_heading()),
                (
                    -1isize,
                    "<FX input> (no support for MIDI clock sources)".to_string(),
                ),
            ]
            .into_iter()
            .chain(
                Reaper::get()
                    .midi_input_devices()
                    .filter(|d| d.is_available())
                    .map(|dev| (dev.id().get() as isize, get_midi_input_device_label(dev))),
            )
            .chain(iter::once((
                -100isize,
                generate_osc_device_heading(osc_devices.len()),
            )))
            .chain(
                osc_devices
                    .enumerate()
                    .map(|(i, dev)| (OSC_INDEX_OFFSET + i as isize, dev.get_list_label(false))),
            )
            .chain([
                (-100isize, String::from("----  Keyboard  ----")),
                (KEYBOARD_INDEX_OFFSET, String::from("Computer keyboard")),
            ]),
        )
    }

    fn invalidate_control_input_combo_box_value(&self) {
        let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
        match self.session().borrow().control_input() {
            ControlInput::Midi(midi_control_input) => match midi_control_input {
                MidiControlInput::FxInput => {
                    b.select_combo_box_item_by_data(-1).unwrap();
                }
                MidiControlInput::Device(dev_id) => b
                    .select_combo_box_item_by_data(dev_id.get() as _)
                    .unwrap_or_else(|_| {
                        b.select_new_combo_box_item(format!("{}. <Unknown>", dev_id.get()));
                    }),
            },
            ControlInput::Osc(osc_device_id) => {
                match App::get()
                    .osc_device_manager()
                    .borrow()
                    .find_index_by_id(&osc_device_id)
                {
                    None => {
                        b.select_new_combo_box_item(format!("<Not present> ({})", osc_device_id));
                    }
                    Some(i) => b
                        .select_combo_box_item_by_data(OSC_INDEX_OFFSET + i as isize)
                        .unwrap(),
                };
            }
            ControlInput::Keyboard => {
                b.select_combo_box_item_by_data(KEYBOARD_INDEX_OFFSET)
                    .unwrap();
            }
        }
    }

    fn invalidate_feedback_output_combo_box(&self) {
        self.invalidate_feedback_output_combo_box_options();
        self.invalidate_feedback_output_combo_box_value();
    }

    fn invalidate_feedback_output_combo_box_options(&self) {
        let b = self
            .view
            .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
        let osc_device_manager = App::get().osc_device_manager();
        let osc_device_manager = osc_device_manager.borrow();
        let osc_devices = osc_device_manager.devices();
        b.fill_combo_box_with_data_small(
            vec![
                (-1isize, "<None>".to_string()),
                (-100isize, generate_midi_device_heading()),
                (-2isize, "<FX output>".to_string()),
            ]
            .into_iter()
            .chain(
                Reaper::get()
                    .midi_output_devices()
                    .filter(|d| d.is_available())
                    .map(|dev| (dev.id().get() as isize, get_midi_output_device_label(dev))),
            )
            .chain(iter::once((
                -100isize,
                generate_osc_device_heading(osc_devices.len()),
            )))
            .chain(
                osc_devices
                    .enumerate()
                    .map(|(i, dev)| (OSC_INDEX_OFFSET + i as isize, dev.get_list_label(true))),
            ),
        )
    }

    fn invalidate_feedback_output_combo_box_value(&self) {
        let b = self
            .view
            .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
        match self.session().borrow().feedback_output() {
            None => {
                b.select_combo_box_item_by_data(-1).unwrap();
            }
            Some(feedback_output) => match feedback_output {
                FeedbackOutput::Midi(o) => match o {
                    MidiDestination::FxOutput => {
                        b.select_combo_box_item_by_data(-2).unwrap();
                    }
                    MidiDestination::Device(dev_id) => b
                        .select_combo_box_item_by_data(dev_id.get() as _)
                        .unwrap_or_else(|_| {
                            b.select_new_combo_box_item(format!("{}. <Unknown>", dev_id.get()));
                        }),
                },
                FeedbackOutput::Osc(osc_device_id) => {
                    match App::get()
                        .osc_device_manager()
                        .borrow()
                        .find_index_by_id(&osc_device_id)
                    {
                        None => {
                            b.select_new_combo_box_item(format!(
                                "<Not present> ({})",
                                osc_device_id
                            ));
                        }
                        Some(i) => b
                            .select_combo_box_item_by_data(OSC_INDEX_OFFSET + i as isize)
                            .unwrap(),
                    }
                }
            },
        }
    }

    fn update_search_expression(&self) {
        let ec = self
            .view
            .require_control(root::ID_HEADER_SEARCH_EDIT_CONTROL);
        let text = ec.text().unwrap_or_else(|_| "".to_string());
        self.main_state
            .borrow_mut()
            .search_expression
            .set_with_initiator(
                SearchExpression::new(&text),
                Some(root::ID_HEADER_SEARCH_EDIT_CONTROL),
            );
    }

    fn invalidate_search_expression(&self, initiator: Option<u32>) {
        let main_state = self.main_state.borrow();
        let search_expression = main_state.search_expression.get_ref().to_string();
        self.view
            .require_control(root::ID_CLEAR_SEARCH_BUTTON)
            .set_enabled(!search_expression.is_empty());
        if initiator != Some(root::ID_HEADER_SEARCH_EDIT_CONTROL) {
            self.view
                .require_control(root::ID_HEADER_SEARCH_EDIT_CONTROL)
                .set_text(search_expression);
        }
    }

    fn update_control_input(&self) {
        let control_input = {
            let b = self.view.require_control(root::ID_CONTROL_DEVICE_COMBO_BOX);
            match b.selected_combo_box_item_data() {
                -1 => Ok(ControlInput::Midi(MidiControlInput::FxInput)),
                KEYBOARD_INDEX_OFFSET => Ok(ControlInput::Keyboard),
                osc_dev_index if osc_dev_index >= OSC_INDEX_OFFSET => {
                    if let Some(dev) = App::get()
                        .osc_device_manager()
                        .borrow()
                        .find_device_by_index((osc_dev_index - OSC_INDEX_OFFSET) as usize)
                    {
                        Ok(ControlInput::Osc(*dev.id()))
                    } else {
                        Err(())
                    }
                }
                midi_dev_id if midi_dev_id >= 0 => {
                    let dev_id = MidiInputDeviceId::new(midi_dev_id as _);
                    Ok(ControlInput::Midi(MidiControlInput::Device(dev_id)))
                }
                _ => Err(()),
            }
        };
        if let Ok(control_input) = control_input {
            self.session().borrow_mut().control_input.set(control_input);
        } else {
            // This is most likely a section entry. Selection is not allowed.
            self.invalidate_control_input_combo_box_value();
        }
    }

    fn update_feedback_output(&self) {
        let feedback_output = {
            let b = self
                .view
                .require_control(root::ID_FEEDBACK_DEVICE_COMBO_BOX);
            match b.selected_combo_box_item_data() {
                -2 => Ok(Some(FeedbackOutput::Midi(MidiDestination::FxOutput))),
                -1 => Ok(None),
                osc_dev_index if osc_dev_index >= OSC_INDEX_OFFSET => {
                    if let Some(dev) = App::get()
                        .osc_device_manager()
                        .borrow()
                        .find_device_by_index((osc_dev_index - OSC_INDEX_OFFSET) as usize)
                    {
                        Ok(Some(FeedbackOutput::Osc(*dev.id())))
                    } else {
                        Err(())
                    }
                }
                midi_dev_id if midi_dev_id >= 0 => {
                    let dev_id = MidiOutputDeviceId::new(midi_dev_id as _);
                    Ok(Some(FeedbackOutput::Midi(MidiDestination::Device(dev_id))))
                }
                _ => Err(()),
            }
        };
        if let Ok(feedback_output) = feedback_output {
            self.session()
                .borrow_mut()
                .feedback_output
                .set(feedback_output);
        } else {
            // This is most likely a section entry. Selection is not allowed.
            self.invalidate_feedback_output_combo_box_value();
        }
    }

    fn update_compartment(&self, compartment: MappingCompartment) {
        let mut main_state = self.main_state.borrow_mut();
        main_state.stop_filter_learning();
        main_state.active_compartment.set(compartment);
    }

    fn remove_group(&self) {
        let id = match self
            .main_state
            .borrow()
            .displayed_group_for_active_compartment()
        {
            Some(GroupFilter(id)) if !id.is_default() => id,
            _ => return,
        };
        let compartment = self.active_compartment();
        let delete_mappings_result = if self
            .session()
            .borrow()
            .group_contains_mappings(compartment, id)
        {
            let msg = "Do you also want to delete all mappings in that group? If you choose no, they will be automatically moved to the default group.";
            self.view
                .require_window()
                .ask_yes_no_or_cancel("ReaLearn", msg)
        } else {
            Some(false)
        };
        if let Some(delete_mappings) = delete_mappings_result {
            self.main_state
                .borrow_mut()
                .set_displayed_group_for_active_compartment(Some(GroupFilter(GroupId::default())));
            self.session()
                .borrow_mut()
                .remove_group(compartment, id, delete_mappings);
        }
    }

    fn edit_group(&self) {
        let compartment = self.active_compartment();
        let weak_group = match self
            .main_state
            .borrow()
            .displayed_group_for_active_compartment()
        {
            Some(GroupFilter(id)) => {
                if id.is_default() {
                    Rc::downgrade(self.session().borrow().default_group(compartment))
                } else {
                    let session = self.session();
                    let session = session.borrow();
                    let group = session
                        .find_group_by_id(compartment, id)
                        .expect("group not existing");
                    Rc::downgrade(group)
                }
            }
            _ => return,
        };
        let panel = GroupPanel::new(self.session.clone(), weak_group);
        let shared_panel = Rc::new(panel);
        if let Some(already_open_panel) =
            self.group_panel.borrow_mut().replace(shared_panel.clone())
        {
            already_open_panel.close();
        }
        shared_panel.open(self.view.require_window());
    }

    fn update_group(&self) {
        let compartment = self.active_compartment();
        let group_filter = match self
            .view
            .require_control(root::ID_GROUP_COMBO_BOX)
            .selected_combo_box_item_data()
        {
            -1 => None,
            i if i >= 0 => {
                let session = self.session();
                let session = session.borrow();
                let group = session
                    .find_group_by_index_sorted(compartment, i as usize)
                    .expect("group not existing")
                    .borrow();
                Some(GroupFilter(group.id()))
            }
            _ => unreachable!(),
        };
        self.main_state
            .borrow_mut()
            .set_displayed_group_for_active_compartment(group_filter);
    }

    fn update_preset_auto_load_mode(&self) {
        let compartment = MappingCompartment::MainMappings;
        self.main_state.borrow_mut().stop_filter_learning();
        let mode = self
            .view
            .require_control(root::ID_AUTO_LOAD_COMBO_BOX)
            .selected_combo_box_item_index()
            .try_into()
            .expect("invalid preset auto-load mode");
        let session = self.session();
        if mode != MainPresetAutoLoadMode::Off {
            {
                if session.borrow().compartment_or_preset_is_dirty(compartment)
                    && !self
                        .view
                        .require_window()
                        .confirm("ReaLearn", COMPARTMENT_CHANGES_WARNING_TEXT)
                {
                    self.invalidate_preset_auto_load_mode_combo_box();
                    return;
                }
            }
            self.panel_manager()
                .borrow_mut()
                .hide_all_with_compartment(compartment);
        }
        self.session()
            .borrow_mut()
            .activate_main_preset_auto_load_mode(mode);
    }

    fn update_preset(&self) {
        self.main_state.borrow_mut().stop_filter_learning();
        let session = self.session();
        let compartment = self.active_compartment();
        let preset_manager = App::get().preset_manager(compartment);
        let compartment_is_dirty = session.borrow().compartment_or_preset_is_dirty(compartment);
        if compartment_is_dirty
            && !self
                .view
                .require_window()
                .confirm("ReaLearn", COMPARTMENT_CHANGES_WARNING_TEXT)
        {
            self.invalidate_preset_combo_box_value();
            return;
        }
        let preset_id = match self
            .view
            .require_control(root::ID_PRESET_COMBO_BOX)
            .selected_combo_box_item_data()
        {
            -1 => None,
            i if i >= 0 => preset_manager.find_id_by_index(i as usize),
            _ => unreachable!(),
        };
        let mut session = session.borrow_mut();
        match compartment {
            MappingCompartment::ControllerMappings => {
                session.activate_controller_preset(preset_id).unwrap();
            }
            MappingCompartment::MainMappings => session.activate_main_preset(preset_id).unwrap(),
        };
    }

    fn mappings_are_read_only(&self) -> bool {
        self.session()
            .borrow()
            .mappings_are_read_only(self.active_compartment())
    }

    fn invalidate_learn_many_button(&self) {
        let is_learning = self.session().borrow().is_learning_many_mappings();
        let learn_button_text = if is_learning { "Stop" } else { "Learn many" };
        let button = self
            .view
            .require_control(root::ID_LEARN_MANY_MAPPINGS_BUTTON);
        button.set_text(learn_button_text);
        let enabled = !(self.active_compartment() == MappingCompartment::MainMappings
            && self.session().borrow().main_preset_auto_load_is_active());
        button.set_enabled(enabled);
    }

    fn invalidate_add_one_button(&self) {
        self.view
            .require_control(root::ID_ADD_MAPPING_BUTTON)
            .set_enabled(!self.mappings_are_read_only());
    }

    fn invalidate_source_filter_buttons(&self) {
        let main_state = self.main_state.borrow();
        self.invalidate_filter_buttons(
            main_state.is_learning_source_filter.get(),
            main_state.source_filter.get_ref().is_some(),
            "Filter source",
            root::ID_FILTER_BY_SOURCE_BUTTON,
            root::ID_CLEAR_SOURCE_FILTER_BUTTON,
        );
    }

    fn invalidate_target_filter_buttons(&self) {
        let main_state = self.main_state.borrow();
        self.invalidate_filter_buttons(
            main_state.is_learning_target_filter.get(),
            main_state.target_filter.get_ref().is_some(),
            "Filter target",
            root::ID_FILTER_BY_TARGET_BUTTON,
            root::ID_CLEAR_TARGET_FILTER_BUTTON,
        );
    }

    fn invalidate_filter_buttons(
        &self,
        is_learning: bool,
        is_set: bool,
        learn_text: &str,
        learn_button_id: u32,
        clear_button_id: u32,
    ) {
        let learn_button_text = if is_learning { "Stop" } else { learn_text };
        self.view
            .require_control(learn_button_id)
            .set_text(learn_button_text);
        self.view
            .require_control(clear_button_id)
            .set_enabled(is_set);
    }

    pub fn import_from_clipboard(&self) -> Result<(), Box<dyn std::error::Error>> {
        let text =
            get_text_from_clipboard().ok_or_else(|| "Couldn't read from clipboard.".to_string())?;
        let plugin_parameters = self
            .plugin_parameters
            .upgrade()
            .expect("plugin params gone");
        let res = {
            let session = self.session();
            let session = session.borrow();
            let compartment_in_session = session.compartment_in_session(self.active_compartment());
            deserialize_data_object(&text, &compartment_in_session)?
        };
        match res.value {
            DataObject::Session(Envelope { value: d}) => {
                if self.view.require_window().confirm(
                    "ReaLearn",
                    "Do you want to continue replacing the complete ReaLearn session with the data in the clipboard?",
                ) {
                    plugin_parameters.apply_session_data(&*d);
                }
            }
            DataObject::ClipMatrix(Envelope { value }) => {
                let old_matrix_label = match self.session().borrow().instance_state().borrow().clip_matrix_ref() {
                    None => EMPTY_CLIP_MATRIX_LABEL.to_owned(),
                    Some(r) => match r {
                        ClipMatrixRef::Own(m) => {
                            get_clip_matrix_label(m.column_count())
                        }
                        ClipMatrixRef::Foreign(instance_id) => {
                            format!("clip matrix reference (to instance {})", instance_id)
                        }
                    },
                };
                let new_matrix_label = match &*value {
                    None => EMPTY_CLIP_MATRIX_LABEL.to_owned(),
                    Some(m) => get_clip_matrix_label(m.columns.as_ref().map(|c|c.len()).unwrap_or(0))
                };
                if self.view.require_window().confirm(
                    "ReaLearn",
                    format!("Do you want to replace the current {} with the {} in the clipboard?", old_matrix_label, new_matrix_label),
                ) {
                    let session = self.session();
                    let session = session.borrow();
                    let mut instance_state = session.instance_state().borrow_mut();
                    if let Some(matrix) = *value {
                        BackboneState::get().get_or_insert_owned_clip_matrix_from_instance_state(&mut instance_state).load(matrix)?;
                    } else {
                        BackboneState::get().clear_clip_matrix_from_instance_state(&mut instance_state);
                    }
                }
            }
            DataObject::MainCompartment(Envelope {value}) => {
                let compartment = MappingCompartment::MainMappings;
                self.import_compartment(compartment, value);
                self.update_compartment(compartment);
            }
            DataObject::ControllerCompartment(Envelope {value}) => {
                let compartment = MappingCompartment::ControllerMappings;
                self.import_compartment(compartment, value);
                self.update_compartment(compartment);
            }
            DataObject::Mappings{..} => {
                return Err("The clipboard contains just a lose collection of mappings. Please import them using the context menus.".into())
            }
            DataObject::Mapping{..} => {
                return Err("The clipboard contains just one single mapping. Please import it using the context menus.".into())
            }
            _ => {
                return Err("The clipboard contains only a part of a mapping. Please import it using the context menus in the mapping area.".into())
            }
        }
        if !res.annotations.is_empty() {
            notify_processing_result(
                "Import from clipboard",
                res.annotations.into_iter().map(|a| a.to_string()).collect(),
            );
        }
        Ok(())
    }

    fn import_compartment(&self, compartment: MappingCompartment, data: Box<CompartmentModelData>) {
        if self.view.require_window().confirm(
            "ReaLearn",
            format!(
                "Do you want to continue replacing the {} with the data in the clipboard?",
                compartment
            ),
        ) {
            let session = self.session();
            let mut session = session.borrow_mut();
            // For now, let's assume that the imported data is always tailored to the running
            // ReaLearn version.
            let version = App::version();
            match data.to_model(Some(version), compartment) {
                Ok(model) => {
                    session.import_compartment(compartment, Some(model));
                }
                Err(e) => {
                    self.view.require_window().alert("ReaLearn", e);
                }
            }
        }
    }

    pub fn export_to_clipboard(&self) -> Result<(), Box<dyn Error>> {
        let menu_bar = MenuBar::new_popup_menu();
        enum MenuAction {
            None,
            ExportSession(SerializationFormat),
            ExportClipMatrix(SerializationFormat),
            ExportCompartment(SerializationFormat),
        }
        impl Default for MenuAction {
            fn default() -> Self {
                Self::None
            }
        }
        let compartment = self.active_compartment();
        let pure_menu = {
            use swell_ui::menu_tree::*;
            let entries = vec![
                item("Export session as JSON", || {
                    MenuAction::ExportSession(SerializationFormat::JsonDataObject)
                }),
                item("Export clip matrix as JSON", || {
                    MenuAction::ExportClipMatrix(SerializationFormat::JsonDataObject)
                }),
                item("Export clip matrix as Lua", || {
                    MenuAction::ExportClipMatrix(SerializationFormat::LuaApiObject(
                        ConversionStyle::Minimal,
                    ))
                }),
                item(format!("Export {} as JSON", compartment), || {
                    MenuAction::ExportCompartment(SerializationFormat::JsonDataObject)
                }),
                item(format!("Export {} as Lua", compartment), || {
                    MenuAction::ExportCompartment(SerializationFormat::LuaApiObject(
                        ConversionStyle::Minimal,
                    ))
                }),
                item(
                    format!("Export {} as Lua (include default values)", compartment),
                    || {
                        MenuAction::ExportCompartment(SerializationFormat::LuaApiObject(
                            ConversionStyle::IncludeDefaultValues,
                        ))
                    },
                ),
            ];
            let mut root_menu = root_menu(entries);
            root_menu.index(1);
            fill_menu(menu_bar.menu(), &root_menu);
            root_menu
        };
        // Open menu
        let location = Window::cursor_pos();
        let result_index = match self
            .view
            .require_window()
            .open_popup_menu(menu_bar.menu(), location)
        {
            None => return Ok(()),
            Some(i) => i,
        };
        let result = pure_menu
            .find_item_by_id(result_index)
            .expect("selected menu item not found")
            .invoke_handler();
        // Execute action
        match result {
            MenuAction::None => {}
            MenuAction::ExportSession(_) => {
                let plugin_parameters = self
                    .plugin_parameters
                    .upgrade()
                    .expect("plugin params gone");
                let session_data = plugin_parameters.create_session_data();
                let data_object = DataObject::Session(Envelope {
                    value: Box::new(session_data),
                });
                let json = serialize_data_object_to_json(data_object).unwrap();
                copy_text_to_clipboard(json);
            }
            MenuAction::ExportClipMatrix(format) => {
                let matrix = self
                    .session()
                    .borrow()
                    .instance_state()
                    .borrow()
                    .owned_clip_matrix()
                    .map(|matrix| matrix.save());
                let envelope = Envelope {
                    value: Box::new(matrix),
                };
                let data_object = DataObject::ClipMatrix(envelope);
                let text = serialize_data_object(data_object, format)?;
                copy_text_to_clipboard(text);
            }
            MenuAction::ExportCompartment(format) => {
                let session = self.session();
                let session = session.borrow();
                let model = session.extract_compartment_model(compartment);
                let data = CompartmentModelData::from_model(&model);
                let envelope = Envelope {
                    value: Box::new(data),
                };
                let data_object = match compartment {
                    MappingCompartment::ControllerMappings => {
                        DataObject::ControllerCompartment(envelope)
                    }
                    MappingCompartment::MainMappings => DataObject::MainCompartment(envelope),
                };
                let text = serialize_data_object(data_object, format)?;
                copy_text_to_clipboard(text);
            }
        };
        Ok(())
    }

    fn notify_user_on_error(&self, result: Result<(), Box<dyn Error>>) {
        if let Err(e) = result {
            self.view.require_window().alert("ReaLearn", e.to_string());
        }
    }

    fn delete_active_preset(&self) -> Result<(), &'static str> {
        if !self
            .view
            .require_window()
            .confirm("ReaLearn", "Do you really want to remove this preset?")
        {
            return Ok(());
        }
        let session = self.session();
        let mut session = session.borrow_mut();
        let compartment = self.active_compartment();
        let mut preset_manager = App::get().preset_manager(compartment);
        let active_preset_id = session
            .active_preset_id(compartment)
            .ok_or("no preset selected")?
            .to_string();
        match compartment {
            MappingCompartment::ControllerMappings => session.activate_controller_preset(None)?,
            MappingCompartment::MainMappings => session.activate_main_preset(None)?,
        };
        preset_manager.remove_preset(&active_preset_id)?;
        Ok(())
    }

    fn reload_all_presets(&self) {
        let _ = App::get()
            .controller_preset_manager()
            .borrow_mut()
            .load_presets();
        let _ = App::get().main_preset_manager().borrow_mut().load_presets();
    }

    fn make_mappings_project_independent_if_desired(&self) {
        let session = self.session();
        let compartment = self.active_compartment();
        if session
            .borrow()
            .mappings_have_project_references(compartment)
            && self.ask_user_if_project_independence_desired()
        {
            session
                .borrow_mut()
                .make_mappings_project_independent(compartment);
        }
    }

    fn save_active_preset(&self) -> Result<(), &'static str> {
        self.make_mappings_project_independent_if_desired();
        let session = self.session();
        let mut session = session.borrow_mut();
        let compartment = self.active_compartment();
        let preset_id = session
            .active_preset_id(compartment)
            .ok_or("no active preset")?;
        let compartment_model = session.extract_compartment_model(compartment);
        match compartment {
            MappingCompartment::ControllerMappings => {
                let preset_manager = App::get().controller_preset_manager();
                let mut controller_preset = preset_manager
                    .find_by_id(preset_id)
                    .ok_or("controller preset not found")?;
                controller_preset.update_realearn_data(compartment_model);
                preset_manager
                    .borrow_mut()
                    .update_preset(controller_preset)?;
            }
            MappingCompartment::MainMappings => {
                let preset_manager = App::get().main_preset_manager();
                let mut main_preset = preset_manager
                    .find_by_id(preset_id)
                    .ok_or("main preset not found")?;
                main_preset.update_data(compartment_model);
                preset_manager.borrow_mut().update_preset(main_preset)?;
            }
        };
        session.compartment_is_dirty[compartment].set(false);
        Ok(())
    }

    fn change_session_id(&self) {
        let current_session_id = { self.session().borrow().id.get_ref().clone() };
        let new_session_id = match dialog_util::prompt_for("Session ID", &current_session_id) {
            None => return,
            Some(n) => n,
        };
        if new_session_id.trim().is_empty() {
            return;
        }
        if new_session_id == current_session_id {
            return;
        }
        if App::get().has_session(&new_session_id) {
            self.view.require_window().alert(
                "ReaLearn",
                "There's another open ReaLearn session which already has this session ID!",
            );
            return;
        }
        let session = self.session();
        let mut session = session.borrow_mut();
        if new_session_id.is_empty() {
            session.reset_id();
        } else {
            session.id.set(new_session_id);
        }
    }

    /// Don't borrow the session while calling this!
    fn ask_user_if_project_independence_desired(&self) -> bool {
        let msg = "Some of the mappings have references to this particular project. This usually doesn't make too much sense for a preset that's supposed to be reusable among different projects. Do you want ReaLearn to automatically adjust the mappings so that track targets refer to tracks by their position and FX targets relate to whatever FX is currently focused?";
        self.view.require_window().confirm("ReaLearn", msg)
    }

    fn save_as_preset(&self) -> Result<(), &'static str> {
        let preset_name = match dialog_util::prompt_for("Preset name", "") {
            None => return Ok(()),
            Some(n) => n,
        };
        if preset_name.trim().is_empty() {
            return Ok(());
        }
        self.make_mappings_project_independent_if_desired();
        let session = self.session();
        let mut session = session.borrow_mut();
        let compartment = self.active_compartment();
        let preset_id = slug::slugify(&preset_name);
        let compartment_model = session.extract_compartment_model(compartment);
        match compartment {
            MappingCompartment::ControllerMappings => {
                let custom_data = session
                    .active_controller_preset()
                    .map(|c| c.custom_data().clone())
                    .unwrap_or_default();
                let controller = ControllerPreset::new(
                    preset_id.clone(),
                    preset_name,
                    compartment_model,
                    custom_data,
                );
                App::get()
                    .controller_preset_manager()
                    .borrow_mut()
                    .add_preset(controller)?;
                session.activate_controller_preset(Some(preset_id))?;
            }
            MappingCompartment::MainMappings => {
                let main_preset =
                    MainPreset::new(preset_id.clone(), preset_name, compartment_model);
                App::get()
                    .main_preset_manager()
                    .borrow_mut()
                    .add_preset(main_preset)?;
                session.activate_main_preset(Some(preset_id))?;
            }
        };
        Ok(())
    }

    fn reset(&self) {
        self.main_state
            .borrow_mut()
            .set_displayed_group_for_active_compartment(Some(GroupFilter(GroupId::default())));
        if let Some(already_open_panel) = self.group_panel.borrow().as_ref() {
            already_open_panel.close();
        }
        self.group_panel.replace(None);
        self.invalidate_all_controls();
    }

    fn log_debug_info(&self) {
        let session = self.session();
        let session = session.borrow();
        session.log_debug_info();
        App::get().log_debug_info(session.id());
    }

    fn open_user_guide_offline(&self) {
        let user_guide_pdf = App::realearn_data_dir_path().join("doc/realearn-user-guide.pdf");
        if open::that(user_guide_pdf).is_err() {
            self.view.require_window().alert(
                "ReaLearn",
                "Couldn't open offline user guide. Please try the online version!",
            )
        }
    }

    fn open_user_guide_online(&self) {
        open_in_browser("https://github.com/helgoboss/realearn/blob/master/doc/user-guide.adoc");
    }

    fn donate(&self) {
        open_in_browser("https://paypal.me/helgoboss");
    }

    fn open_forum(&self) {
        open_in_browser("https://forum.cockos.com/showthread.php?t=178015");
    }

    fn contact_developer(&self) {
        open_in_browser("mailto:info@helgoboss.org");
    }

    fn open_website(&self) {
        open_in_browser("https://www.helgoboss.org/projects/realearn/");
    }

    fn register_listeners(self: SharedView<Self>) {
        let shared_session = self.session();
        let session = shared_session.borrow();
        self.when(session.everything_changed(), |view, _| {
            view.reset();
        });
        self.when(
            session
                .let_matched_events_through
                .changed()
                .merge(session.let_unmatched_events_through.changed()),
            |view, _| {
                view.invalidate_let_through_controls();
            },
        );
        self.when(session.learn_many_state_changed(), |view, _| {
            view.invalidate_all_controls();
        });
        self.when(session.control_input.changed(), |view, _| {
            view.invalidate_control_input_combo_box();
            view.invalidate_let_through_controls();
            let shared_session = view.session();
            let mut session = shared_session.borrow_mut();
            let control_input = session.control_input();
            if control_input.is_midi_device() && !reaper_supports_global_midi_filter() {
                session.let_matched_events_through.set(true);
                session.let_unmatched_events_through.set(true);
            }
            if session.auto_correct_settings.get() {
                session
                    .send_feedback_only_if_armed
                    .set(control_input == ControlInput::Midi(MidiControlInput::FxInput));
            }
        });
        self.when(session.feedback_output.changed(), |view, _| {
            view.invalidate_feedback_output_combo_box()
        });
        let main_state = self.main_state.borrow();
        self.when(
            main_state.displayed_group_for_any_compartment_changed(),
            |view, _| {
                view.invalidate_group_controls();
            },
        );
        self.when(
            main_state.search_expression.changed_with_initiator(),
            |view, initiator| {
                view.invoke_programmatically(|| {
                    view.invalidate_search_expression(initiator);
                });
            },
        );
        self.when(
            main_state
                .is_learning_target_filter
                .changed()
                .merge(main_state.target_filter.changed()),
            |view, _| {
                view.invalidate_target_filter_buttons();
            },
        );
        self.when(
            main_state
                .is_learning_source_filter
                .changed()
                .merge(main_state.source_filter.changed()),
            |view, _| {
                view.invalidate_source_filter_buttons();
            },
        );
        self.when(main_state.active_compartment.changed(), |view, _| {
            view.invalidate_all_controls();
        });
        self.when(session.main_preset_auto_load_mode.changed(), |view, _| {
            view.invalidate_all_controls();
        });
        self.when(session.group_list_changed(), |view, _| {
            view.invalidate_group_controls();
        });
        when(
            App::get()
                .controller_preset_manager()
                .borrow()
                .changed()
                .merge(App::get().main_preset_manager().borrow().changed())
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(&self))
        .do_async(move |view, _| {
            view.invalidate_preset_controls();
        });
        when(
            App::get()
                .osc_device_manager()
                .borrow()
                .changed()
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(&self))
        .do_async(move |view, _| {
            view.invalidate_control_input_combo_box();
            view.invalidate_feedback_output_combo_box();
        });
        // Enables/disables save button depending on dirty state.
        when(
            session.compartment_is_dirty[MappingCompartment::ControllerMappings]
                .changed()
                .merge(session.compartment_is_dirty[MappingCompartment::MainMappings].changed())
                .take_until(self.view.closed()),
        )
        .with(Rc::downgrade(&self))
        .do_sync(move |view, _| {
            view.invalidate_preset_buttons();
        });
    }

    fn when<I: Send + Sync + Clone + 'static>(
        self: &SharedView<Self>,
        event: impl LocalObservable<'static, Item = I, Err = ()> + 'static,
        reaction: impl Fn(SharedView<Self>, I) + 'static + Clone,
    ) {
        when(event.take_until(self.view.closed()))
            .with(Rc::downgrade(self))
            .do_sync(reaction);
    }

    fn is_invoked_programmatically(&self) -> bool {
        self.is_invoked_programmatically.get()
    }
}

impl View for HeaderPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_HEADER_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, _window: Window) -> bool {
        self.fill_all_controls();
        self.invalidate_all_controls();
        self.invalidate_search_expression(None);
        self.register_listeners();
        true
    }

    fn closed(self: SharedView<Self>, _window: Window) {
        self.main_state.borrow_mut().stop_filter_learning();
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_GROUP_ADD_BUTTON => self.add_group(),
            root::ID_GROUP_DELETE_BUTTON => self.remove_group(),
            root::ID_GROUP_EDIT_BUTTON => self.edit_group(),
            root::ID_ADD_MAPPING_BUTTON => self.add_mapping(),
            root::ID_LEARN_MANY_MAPPINGS_BUTTON => {
                self.toggle_learn_many_mappings();
            }
            root::ID_FILTER_BY_SOURCE_BUTTON => self.toggle_learn_source_filter(),
            root::ID_FILTER_BY_TARGET_BUTTON => self.toggle_learn_target_filter(),
            root::ID_CLEAR_SOURCE_FILTER_BUTTON => self.clear_source_filter(),
            root::ID_CLEAR_TARGET_FILTER_BUTTON => self.clear_target_filter(),
            root::ID_CLEAR_SEARCH_BUTTON => self.clear_search_expression(),
            root::ID_IMPORT_BUTTON => {
                if let Err(error) = self.import_from_clipboard() {
                    self.view
                        .require_window()
                        .alert("ReaLearn", error.to_string());
                }
            }
            root::ID_EXPORT_BUTTON => {
                self.notify_user_on_error(self.export_to_clipboard());
            }
            root::ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX => {
                self.update_let_matched_events_through()
            }
            root::ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX => {
                self.update_let_unmatched_events_through()
            }
            root::ID_PRESET_DELETE_BUTTON => {
                self.delete_active_preset().unwrap();
            }
            root::ID_PRESET_SAVE_AS_BUTTON => {
                self.save_as_preset().unwrap();
            }
            root::ID_PRESET_SAVE_BUTTON => {
                self.save_active_preset().unwrap();
            }
            root::ID_PROJECTION_BUTTON => {
                self.companion_app_presenter.show_app_info();
            }
            root::ID_CONTROLLER_COMPARTMENT_RADIO_BUTTON => {
                self.update_compartment(MappingCompartment::ControllerMappings)
            }
            root::ID_MAIN_COMPARTMENT_RADIO_BUTTON => {
                self.update_compartment(MappingCompartment::MainMappings)
            }
            _ => {}
        }
    }

    fn option_selected(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_CONTROL_DEVICE_COMBO_BOX => self.update_control_input(),
            root::ID_FEEDBACK_DEVICE_COMBO_BOX => self.update_feedback_output(),
            root::ID_GROUP_COMBO_BOX => self.update_group(),
            root::ID_AUTO_LOAD_COMBO_BOX => self.update_preset_auto_load_mode(),
            root::ID_PRESET_COMBO_BOX => self.update_preset(),
            _ => unreachable!(),
        }
    }

    fn edit_control_changed(self: SharedView<Self>, resource_id: u32) -> bool {
        if self.is_invoked_programmatically() {
            // We don't want to continue if the edit control change was not caused by the user.
            // Although the edit control text is changed programmatically, it also triggers the
            // change handler. Ignore it! Most of those events are filtered out already
            // by the dialog proc reentrancy check, but this one is not because the
            // dialog proc is not reentered - we are just reacting (async) to a change.
            return false;
        }
        match resource_id {
            root::ID_HEADER_SEARCH_EDIT_CONTROL => self.update_search_expression(),
            _ => unreachable!(),
        }
        true
    }

    fn context_menu_wanted(self: SharedView<Self>, location: Point<Pixels>) {
        let _ = self.open_context_menu(location);
    }
}

fn get_midi_input_device_label(dev: MidiInputDevice) -> String {
    get_midi_device_label(
        dev.name(),
        dev.id().get(),
        MidiDeviceStatus::from_flags(dev.is_open(), dev.is_connected()),
    )
}

fn get_midi_output_device_label(dev: MidiOutputDevice) -> String {
    get_midi_device_label(
        dev.name(),
        dev.id().get(),
        MidiDeviceStatus::from_flags(dev.is_open(), dev.is_connected()),
    )
}

#[derive(Display)]
enum MidiDeviceStatus {
    #[display(fmt = " <disconnected>")]
    Disconnected,
    #[display(fmt = " <connected but disabled>")]
    ConnectedButDisabled,
    #[display(fmt = "")]
    Connected,
}

impl MidiDeviceStatus {
    fn from_flags(open: bool, connected: bool) -> MidiDeviceStatus {
        use MidiDeviceStatus::*;
        match (open, connected) {
            (false, false) => Disconnected,
            (false, true) => ConnectedButDisabled,
            // Shouldn't happen but cope with it.
            (true, false) => Disconnected,
            (true, true) => Connected,
        }
    }
}

fn get_midi_device_label(name: ReaperString, raw_id: u8, status: MidiDeviceStatus) -> String {
    format!(
        "{}. {}{}",
        raw_id,
        // Here we don't rely on the string to be UTF-8 because REAPER doesn't have influence on
        // how MIDI devices encode their name. Indeed a user reported an error related to that:
        // https://github.com/helgoboss/realearn/issues/78
        name.into_inner().to_string_lossy(),
        status
    )
}

impl Drop for HeaderPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping header panel...");
    }
}

fn generate_midi_device_heading() -> String {
    "----  MIDI  ----".to_owned()
}

fn generate_osc_device_heading(device_count: usize) -> String {
    format!(
        "----  OSC  ----{}",
        if device_count == 0 {
            " (add devices via right-click menu)"
        } else {
            ""
        }
    )
}

fn edit_preset_link_fx_id(old_fx_id: FxId) {
    let new_fx_id = match edit_fx_id(&old_fx_id) {
        Ok(d) => d,
        Err(EditFxIdError::Cancelled) => return,
        res => res.unwrap(),
    };
    App::get()
        .preset_link_manager()
        .borrow_mut()
        .update_fx_id(old_fx_id, new_fx_id);
}

fn remove_preset_link(fx_id: FxId) {
    App::get()
        .preset_link_manager()
        .borrow_mut()
        .remove_link(&fx_id);
}

fn link_to_preset(fx_id: FxId, preset_id: String) {
    App::get()
        .preset_link_manager()
        .borrow_mut()
        .link_preset_to_fx(preset_id, fx_id);
}

fn edit_fx_id(fx_id: &FxId) -> Result<FxId, EditFxIdError> {
    let csv = Reaper::get()
        .medium_reaper()
        .get_user_inputs(
            "ReaLearn",
            3,
            "FX name,FX file name,FX preset name,separator=;,extrawidth=80",
            format!(
                "{};{};{}",
                fx_id.name(),
                fx_id.file_name(),
                fx_id.preset_name()
            ),
            512,
        )
        .ok_or(EditFxIdError::Cancelled)?;
    let split: Vec<_> = csv.to_str().split(';').collect();
    if let [name, file_name, preset_name] = split.as_slice() {
        let new_fx_id = FxId {
            name: name.to_string(),
            file_name: file_name.to_string(),
            preset_name: preset_name.to_string(),
        };
        Ok(new_fx_id)
    } else {
        Err(EditFxIdError::Unexpected("couldn't split"))
    }
}

#[derive(Debug)]
enum EditFxIdError {
    Cancelled,
    Unexpected(&'static str),
}

fn edit_new_osc_device() {
    let dev = match edit_osc_device(OscDevice::default()) {
        Ok(d) => d,
        Err(EditOscDevError::Cancelled) => return,
        res => res.unwrap(),
    };
    App::get()
        .osc_device_manager()
        .borrow_mut()
        .add_device(dev)
        .unwrap();
}

fn edit_existing_osc_device(dev_id: OscDeviceId) {
    let dev = App::get()
        .osc_device_manager()
        .borrow()
        .find_device_by_id(&dev_id)
        .unwrap()
        .clone();
    let dev = match edit_osc_device(dev) {
        Ok(d) => d,
        Err(EditOscDevError::Cancelled) => return,
        res => res.unwrap(),
    };
    App::get()
        .osc_device_manager()
        .borrow_mut()
        .update_device(dev)
        .unwrap();
}

fn remove_osc_device(parent_window: Window, dev_id: OscDeviceId) {
    if !parent_window.confirm(
        "ReaLearn",
        "Do you really want to remove this OSC device? This is a global action. As a consequence, all existing ReaLearn instances which use this device will point to a device that doesn't exist anymore.",
    ) {
        return;
    }
    App::get()
        .osc_device_manager()
        .borrow_mut()
        .remove_device_by_id(dev_id)
        .unwrap();
}

fn edit_compartment_parameter(
    session: SharedSession,
    compartment: MappingCompartment,
    rel_index: u32,
) -> Result<(), &'static str> {
    let batch_index = rel_index / PARAM_BATCH_SIZE;
    let offset = batch_index * PARAM_BATCH_SIZE;
    let range = offset..(offset + PARAM_BATCH_SIZE);
    let current_settings: Vec<_> = {
        let session = session.borrow();
        let session_state = session.state().borrow();
        range
            .clone()
            .map(|i| session_state.get_setting(compartment, i))
            .cloned()
            .collect()
    };
    let modified_settings = edit_compartment_parameter_internal(offset, &current_settings)?;
    session
        .borrow_mut()
        .set_parameter_settings(compartment, range.zip(modified_settings));
    Ok(())
}

#[derive(Debug)]
enum EditOscDevError {
    Cancelled,
    Unexpected(&'static str),
}

/// Pass max 5 settings.
fn edit_compartment_parameter_internal(
    offset: u32,
    settings: &[ParamSetting],
) -> Result<Vec<ParamSetting>, &'static str> {
    let mut captions_csv = (offset..)
        .zip(settings)
        .map(|(i, _)| format!("Param {} name,Value count", i + 1))
        .join(",");
    captions_csv.push_str(",separator=;,extrawidth=80");
    let initial_csv = settings
        .iter()
        .flat_map(|s| {
            [
                s.name.clone(),
                s.value_count.map(|v| v.to_string()).unwrap_or_default(),
            ]
        })
        .join(";");
    let csv = Reaper::get()
        .medium_reaper()
        .get_user_inputs(
            "ReaLearn",
            (settings.len() * 2) as u32,
            captions_csv,
            initial_csv,
            1024,
        )
        .ok_or("cancelled")?;
    let tuples = csv.to_str().split(';').tuples();
    let out_settings: Vec<_> = tuples
        .zip(settings)
        .map(|((name, value_count), old_setting)| ParamSetting {
            key: old_setting.key.clone(),
            name: name.trim().to_owned(),
            value_count: { value_count.parse().ok() },
        })
        .collect();
    if out_settings.len() != settings.len() {
        return Err("unexpected length difference");
    }
    Ok(out_settings)
}

fn edit_osc_device(mut dev: OscDevice) -> Result<OscDevice, EditOscDevError> {
    let csv = Reaper::get()
        .medium_reaper()
        .get_user_inputs(
            "ReaLearn",
            4,
            "Name,Local port (e.g. 7878),Device host (e.g. 192.168.x.y),Device port (e.g. 7878),separator=;,extrawidth=80",
            format!(
                "{};{};{};{}",
                dev.name(),
                dev.local_port().map(|p| p.to_string()).unwrap_or_default(),
                dev.device_host().map(|a| a.to_string()).unwrap_or_default(),
                dev.device_port().map(|p| p.to_string()).unwrap_or_default(),
            ),
            512,
        )
        .ok_or(EditOscDevError::Cancelled)?;
    let splitted: Vec<_> = csv.to_str().split(';').collect();
    if let [name, local_port, device_host, device_port] = splitted.as_slice() {
        dev.set_name(name.to_string());
        dev.set_local_port(local_port.parse::<u16>().ok());
        dev.set_device_host(device_host.parse::<Ipv4Addr>().ok());
        dev.set_device_port(device_port.parse::<u16>().ok());
        Ok(dev)
    } else {
        Err(EditOscDevError::Unexpected("couldn't split"))
    }
}

const COMPARTMENT_CHANGES_WARNING_TEXT: &str = "Mapping/group/parameter changes in this compartment will be lost. Consider to save them first. Do you really want to continue?";

const EMPTY_CLIP_MATRIX_LABEL: &str = "empty clip matrix";

fn get_clip_matrix_label(column_count: usize) -> String {
    format!("clip matrix with {} columns", column_count)
}
