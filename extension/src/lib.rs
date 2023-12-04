use anyhow::{bail, Context, Result};
use ini::{Ini, ParseOption};
use libloading::{Library, Symbol};
use realearn_api::runtime::{HelgoboxApiPointers, HelgoboxApiSession};
use reaper_fluent::{FreeFn, Reaper};
use reaper_low::{PluginContext, TypeSpecificPluginContext};
use reaper_macros::reaper_extension_plugin;
use reaper_medium::{
    reaper_str, AcceleratorBehavior, AcceleratorKeyCode, AddFxBehavior, CommandId, HookCommand,
    OwnedGaccelRegister, ProjectContext, ReaperSession, ReaperStr, TrackDefaultsBehavior,
    TrackFxChainType,
};
use std::error::Error;
use std::ptr::null_mut;
use std::sync::OnceLock;
use std::{cmp, fs};

// Executing Drop not important because extensions always live until REAPER ends.
static EXTENSION: OnceLock<HelgoboxExtension> = OnceLock::new();

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> std::result::Result<(), Box<dyn Error>> {
    let _ = EXTENSION.set(HelgoboxExtension::load(context)?);
    Ok(())
}

type ReaperPluginEntry = unsafe extern "C" fn(
    h_instance: ::reaper_low::raw::HINSTANCE,
    rec: *mut ::reaper_low::raw::reaper_plugin_info_t,
) -> ::std::os::raw::c_int;

struct HelgoboxExtension {
    plugin_library: Option<Library>,
    show_or_hide_playtime_command_id: CommandId,
}

impl HelgoboxExtension {
    pub fn load(context: PluginContext) -> Result<Self> {
        // Do our own thing
        let mut medium_session = ReaperSession::load(context);
        medium_session.plugin_register_add_hook_command::<Self>()?;
        // Register actions
        let show_or_hide_playtime_command_id =
            medium_session.plugin_register_add_command_id(COMMAND_ID_SHOW_HIDE_PLAYTIME)?;
        let gaccel_register = OwnedGaccelRegister::with_key_binding(
            show_or_hide_playtime_command_id,
            ACTION_LABEL_SHOW_HIDE_PLAYTIME,
            AcceleratorBehavior::Shift
                | AcceleratorBehavior::Control
                | AcceleratorBehavior::VirtKey,
            AcceleratorKeyCode::new(b'P' as _),
        );
        medium_session
            .plugin_register_add_gaccel_global_text(gaccel_register)
            .or_else(|gaccel_register| {
                medium_session.plugin_register_add_gaccel(gaccel_register)
            })?;
        // Install reaper-fluent for global use
        let _ = Reaper::install_globally(medium_session);
        // Add toolbar button
        let _ = add_playtime_toolbar_button();
        // Return extension
        let extension = Self {
            plugin_library: eagerly_load_plugin_lib(&context).ok(),
            show_or_hide_playtime_command_id,
        };
        Ok(extension)
    }

    pub fn get() -> &'static HelgoboxExtension {
        EXTENSION
            .get()
            .expect("Helgobox extension not yet initialized")
    }

    fn show_or_hide_playtime(&self) -> Result<()> {
        let plugin_context = Reaper::get().medium_reaper().low().plugin_context();
        let Some(helgobox_api_session) = HelgoboxApiSession::load(plugin_context) else {
            // Project doesn't have any Helgobox instance yet. Add one.
            add_and_show_playtime()?;
            return Ok(());
        };
        let helgobox_instance =
            helgobox_api_session.HB_FindFirstPlaytimeHelgoboxInstanceInProject(null_mut());
        if helgobox_instance == -1 {
            // Project doesn't have any Playtime-enabled Helgobox instance yet. Add one.
            add_and_show_playtime()?;
            return Ok(());
        }
        helgobox_api_session.HB_ShowOrHidePlaytime(helgobox_instance);
        Ok(())
    }

    fn command_invoked(&self, command_id: CommandId) -> Result<bool> {
        match command_id {
            id if id == self.show_or_hide_playtime_command_id => {
                self.show_or_hide_playtime()?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl HookCommand for HelgoboxExtension {
    fn call(command_id: CommandId, _flag: i32) -> bool {
        HelgoboxExtension::get()
            .command_invoked(command_id)
            .expect("command invocation failed")
    }
}

fn add_and_show_playtime() -> Result<()> {
    let mut model = Reaper::get().model_mut();
    let mut project = model.current_project_mut();
    let mut track = project.insert_track_at(0, TrackDefaultsBehavior::OmitDefaultEnvAndFx);
    track.set_name("Playtime");
    track
        .normal_fx_chain_mut()
        .add_fx_by_name("<1751282284", AddFxBehavior::AlwaysAdd)
        .context("Couldn't add Helgobox. Maybe not installed?")?
        .hide_window();
    // The rest needs to be done async because the instance initializes itself async
    // (because FX not yet available when plug-in instantiated).
    // TODO-high Naaah, we need to equip reaper-fluent with something better than this ;)
    Reaper::get().execute_later::<Later>();
    struct Later;
    struct MuchLater;
    impl FreeFn for Later {
        fn call() {
            Reaper::get().execute_later::<MuchLater>();
        }
    }
    impl FreeFn for MuchLater {
        fn call() {
            enable_playtime_for_first_helgobox_instance_and_show_it().unwrap();
        }
    }
    Ok(())
}

fn enable_playtime_for_first_helgobox_instance_and_show_it() -> Result<()> {
    let plugin_context = Reaper::get().medium_reaper().low().plugin_context();
    let helgobox_api_session = HelgoboxApiSession::load(&plugin_context)
        .context("Couldn't load API even after adding Helgobox. Old version?")?;
    let instance_id = helgobox_api_session.HB_FindFirstHelgoboxInstanceInProject(null_mut());
    helgobox_api_session.HB_CreateClipMatrix(instance_id);
    helgobox_api_session.HB_ShowOrHidePlaytime(instance_id);
    Ok(())
}

/// Loads the Helgobox plug-in library eagerly (Justin's idea, awesome!)
fn eagerly_load_plugin_lib(context: &PluginContext) -> Result<Library> {
    // Find correct shared library file
    let resource_path = Reaper::get()
        .medium_reaper()
        .get_resource_path(|path| path.to_path_buf());
    let plugin_library_path = fs::read_dir(resource_path.join("UserPlugins/FX"))?
        .flatten()
        .find_map(|item| {
            let file_type = item.file_type().ok()?;
            if !file_type.is_file() && !file_type.is_symlink() {
                return None;
            }
            let file_name = item.file_name().to_str()?.to_lowercase();
            let extension = if cfg!(target_os = "windows") {
                ".dll"
            } else if cfg!(target_os = "macos") {
                ".vst.dylib"
            } else {
                ".so"
            };
            let matches = file_name.starts_with("realearn") && file_name.ends_with(extension);
            if !matches {
                return None;
            }
            Some(item.path())
        })
        .context("couldn't find plug-in library")?;
    // Load shared library
    let plugin_library = unsafe { Library::new(plugin_library_path)? };
    // Run extension entry point of library
    let reaper_plugin_entry: Symbol<ReaperPluginEntry> =
        unsafe { plugin_library.get(b"ReaperPluginEntry")? };
    let TypeSpecificPluginContext::Extension(ctx) = context.type_specific() else {
        bail!("unexpected plug-in context type for extension");
    };
    let mut original_info_struct = ctx.to_raw();
    unsafe {
        reaper_plugin_entry(context.h_instance(), &mut original_info_struct as *mut _);
    }
    Ok(plugin_library)
}

fn add_playtime_toolbar_button() -> Result<()> {
    // Load toolbar button INI file
    let reaper_menu_ini = Reaper::get()
        .medium_reaper()
        .get_resource_path(|p| p.join("reaper-menu.ini"));
    let mut ini = Ini::load_from_file_opt(
        &reaper_menu_ini,
        ParseOption {
            enabled_quote: false,
            enabled_escape: false,
        },
    )?;
    // Look through existing toolbar buttons
    let toolbar_section = ini
        .section_mut(Some("Main toolbar"))
        .context("couldn't find main toolbar section")?;
    let mut max_item_index = -1i32;
    for (key, value) in toolbar_section.iter() {
        let Some(toolbar_item) = ToolbarItem::parse_from_ini_prop(key, value) else {
            continue;
        };
        if &toolbar_item.command[1..] == COMMAND_ID_SHOW_HIDE_PLAYTIME {
            // Toolbar button exists already
            return Ok(());
        }
        max_item_index = max_item_index.max(toolbar_item.index as _);
    }
    // Add new toolbar button
    let next_item_index = max_item_index + 1;
    toolbar_section.insert(
        format!("item_{next_item_index}"),
        format!("_{COMMAND_ID_SHOW_HIDE_PLAYTIME} {ACTION_LABEL_SHOW_HIDE_PLAYTIME}"),
    );
    ini.write_to_file(&reaper_menu_ini)?;
    Reaper::get().medium_reaper().low().UpdateArrange();
    Reaper::get().medium_reaper().low().UpdateTimeline();
    Ok(())
}

struct ToolbarItem<'a> {
    index: u32,
    command: &'a str,
    desc: &'a str,
}

impl<'a> ToolbarItem<'a> {
    fn parse_from_ini_prop(key: &'a str, value: &'a str) -> Option<Self> {
        let Some(("item", i)) = key.split_once('_') else {
            return None;
        };
        let Some((command, desc)) = value.split_once(' ') else {
            return None;
        };
        let item = ToolbarItem {
            index: i.parse().ok()?,
            command,
            desc,
        };
        Some(item)
    }
}

const COMMAND_ID_SHOW_HIDE_PLAYTIME: &str = "HB_SHOW_HIDE_PLAYTIME";
const ACTION_LABEL_SHOW_HIDE_PLAYTIME: &str = "Show/hide Playtime";
