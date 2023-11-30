use anyhow::{Context, Result};
use reafluent::Reaper;
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

// Executing Drop not important because extensions always live until REAPER ends.
static EXTENSION: OnceLock<HelgoboxExtension> = OnceLock::new();

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> std::result::Result<(), Box<dyn Error>> {
    let _ = EXTENSION.set(HelgoboxExtension::load(context)?);
    Ok(())
}

struct HelgoboxExtension {
    show_or_hide_playtime_command_id: CommandId,
}

impl HelgoboxExtension {
    pub fn load(context: PluginContext) -> Result<Self> {
        let mut medium_session = ReaperSession::load(context);
        medium_session.plugin_register_add_hook_command::<Self>()?;
        // Register actions
        let show_or_hide_playtime_command_id =
            medium_session.plugin_register_add_command_id(reaper_str!("HB_SHOW_HIDE_PLAYTIME"))?;
        medium_session.plugin_register_add_gaccel(OwnedGaccelRegister::without_key_binding(
            show_or_hide_playtime_command_id,
            "Show/hide Playtime",
        ))?;
        let _ = Reaper::install_globally(medium_session);
        let extension = Self {
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
        let reaper = Reaper::get();
        let medium_reaper = reaper.medium_session().reaper();
        let plugin_context = medium_reaper.low().plugin_context();
        let Some(helgobox_api_pointers) = HelgoboxApiPointers::load(&plugin_context) else {
            add_and_show_playtime()?;
            return Ok(());
        };
        let helgobox_api_session = HelgoboxApiSession::new(helgobox_api_pointers);
        let helgobox_instance =
            helgobox_api_session.HB_FindFirstPlaytimeInstanceInProject(null_mut());
        if helgobox_instance == -1 {
            // Project doesn't have any Playtime-enabled Helgobox instance yet. Add one.
            add_and_show_playtime()?;
            return Ok(());
        }
        helgobox_api_session.HB_ShowOrHidePlaytime(helgobox_instance);
        Ok(())
    }
}

impl HookCommand for HelgoboxExtension {
    fn call(command_id: CommandId, _flag: i32) -> bool {
        let extension = HelgoboxExtension::get();
        match command_id {
            id if id == extension.show_or_hide_playtime_command_id => {
                let _ = extension.show_or_hide_playtime();
                true
            }
            _ => false,
        }
    }
}

fn add_and_show_playtime() -> Result<()> {
    let instance_id = Reaper::get()
        .insert_track_at(0, TrackDefaultsBehavior::OmitDefaultEnvAndFx)
        .normal_fx_chain()
        .resolve()
        .expect("must exist")
        .add_fx_by_name("<1751282284", AddFxBehavior::AlwaysAdd)
        .context("Couldn't add Helgobox. Maybe not installed?")?
        .resolve()
        .expect("must exist")
        .get_named_config_param_as_string("INSTANCE_ID", 32)
        .context("Helgobox doesn't expose instance ID. Maybe an older version?")?;
    Ok(())
}
