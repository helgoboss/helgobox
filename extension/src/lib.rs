use anyhow::{Context, Result};
use fragile::Fragile;
use realearn_api::runtime::{HelgoboxApiPointers, HelgoboxApiSession};
use reaper_low::PluginContext;
use reaper_macros::reaper_extension_plugin;
use reaper_medium::{
    reaper_str, AddFxBehavior, CommandId, HookCommand, OwnedGaccelRegister, ProjectContext,
    ReaperSession, TrackDefaultsBehavior, TrackFxChainType,
};
use std::error::Error;
use std::ptr::null_mut;
use std::sync::OnceLock;

static EXTENSION: OnceLock<HelgoboxExtension> = OnceLock::new();

fn extension() -> &'static HelgoboxExtension {
    EXTENSION
        .get()
        .expect("Helgobox extension not yet initialized")
}

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> std::result::Result<(), Box<dyn Error>> {
    let _ = EXTENSION.set(HelgoboxExtension::load(context)?);
    Ok(())
}

struct HelgoboxExtension {
    show_or_hide_playtime_command_id: CommandId,
    reaper_session: Fragile<ReaperSession>,
}

impl HelgoboxExtension {
    pub fn load(context: PluginContext) -> Result<Self> {
        let mut session = ReaperSession::load(context);
        let show_or_hide_playtime_command_id =
            session.plugin_register_add_command_id(reaper_str!("HB_SHOW_HIDE_PLAYTIME"))?;
        session.plugin_register_add_hook_command::<ShowOrHidePlaytimeCommand>()?;
        session.plugin_register_add_gaccel(OwnedGaccelRegister::without_key_binding(
            show_or_hide_playtime_command_id,
            "Show/hide Playtime",
        ))?;
        let extension = Self {
            show_or_hide_playtime_command_id,
            reaper_session: Fragile::new(session),
        };
        Ok(extension)
    }
}

struct ShowOrHidePlaytimeCommand;

impl HookCommand for ShowOrHidePlaytimeCommand {
    fn call(command_id: CommandId, _flag: i32) -> bool {
        if command_id != extension().show_or_hide_playtime_command_id {
            return false;
        }
        let _ = show_or_hide_playtime();
        true
    }
}

fn show_or_hide_playtime() -> Result<()> {
    let reaper = extension().reaper_session.get().reaper();
    let plugin_context = reaper.low().plugin_context();
    let pointers = HelgoboxApiPointers::load(&plugin_context);
    let session = HelgoboxApiSession::new(pointers);
    let instance = session.HB_FindFirstPlaytimeInstanceInProject(null_mut());
    if instance == -1 {
        // Project doesn't have any Playtime-enabled Helgobox instance yet. Add one.
        reaper.insert_track_at_index(0, TrackDefaultsBehavior::OmitDefaultEnvAndFx);
        let new_track = reaper
            .get_track(ProjectContext::CurrentProject, 0)
            .context("track must exist")?;
        unsafe {
            reaper
                .track_fx_add_by_name_add(
                    new_track,
                    "<1751282284",
                    TrackFxChainType::NormalFxChain,
                    AddFxBehavior::AlwaysAdd,
                )
                .context("Couldn't add Helgobox. Maybe not installed?")?;
        }
        // reaper.track_fx_get_named_config_parm_as_string();
        return Ok(());
    }
    session.HB_ShowOrHidePlaytime(instance);
    Ok(())
}
