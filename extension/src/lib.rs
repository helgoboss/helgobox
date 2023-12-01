use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use realearn_api::runtime::{HelgoboxApiPointers, HelgoboxApiSession};
use reaper_fluent::{FreeFn, Reaper};
use reaper_low::{PluginContext, TypeSpecificPluginContext};
use reaper_macros::reaper_extension_plugin;
use reaper_medium::{
    reaper_str, AcceleratorBehavior, AcceleratorKeyCode, AddFxBehavior, CommandId, HookCommand,
    OwnedGaccelRegister, ProjectContext, ReaperSession, TrackDefaultsBehavior, TrackFxChainType,
};
use std::error::Error;
use std::ptr::null_mut;
use std::sync::OnceLock;

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
            medium_session.plugin_register_add_command_id(reaper_str!("HB_SHOW_HIDE_PLAYTIME"))?;
        medium_session.plugin_register_add_gaccel(OwnedGaccelRegister::with_key_binding(
            show_or_hide_playtime_command_id,
            "Show/hide Playtime",
            AcceleratorBehavior::Shift
                | AcceleratorBehavior::Control
                | AcceleratorBehavior::VirtKey,
            AcceleratorKeyCode::new(b'P' as _),
        ))?;
        let _ = Reaper::install_globally(medium_session);
        // Eagerly load plug-in library (Justin's idea, awesome!)
        let resource_path = Reaper::get()
            .medium_reaper()
            .get_resource_path(|path| path.to_path_buf());
        let plugin_library_path = resource_path
            .join("UserPlugins/FX")
            .join("realearn.vst.dylib");
        let plugin_library = unsafe { Library::new(plugin_library_path) }
            .ok()
            .and_then(|lib| {
                let reaper_plugin_entry: Symbol<ReaperPluginEntry> =
                    unsafe { lib.get(b"ReaperPluginEntry").ok()? };
                let TypeSpecificPluginContext::Extension(ctx) = context.type_specific() else {
                    return None;
                };
                let mut original_info_struct = ctx.to_raw();
                unsafe {
                    reaper_plugin_entry(context.h_instance(), &mut original_info_struct as *mut _);
                }
                Some(lib)
            });
        // Return extension
        let extension = Self {
            plugin_library,
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
    Reaper::get()
        .model_mut()
        .current_project_mut()
        .insert_track_at(0, TrackDefaultsBehavior::OmitDefaultEnvAndFx)
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
