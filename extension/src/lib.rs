use anyhow::Result;
use fragile::Fragile;
use realearn_api::runtime::{HelgoboxApiPointers, HelgoboxApiSession};
use reaper_low::PluginContext;
use reaper_macros::reaper_extension_plugin;
use reaper_medium::{reaper_str, CommandId, HookCommand, OwnedGaccelRegister, ReaperSession};
use std::error::Error;
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
    my_command_id: CommandId,
    reaper_session: Fragile<ReaperSession>,
}

impl HelgoboxExtension {
    pub fn load(context: PluginContext) -> Result<Self> {
        let mut session = ReaperSession::load(context);
        let my_command_id =
            session.plugin_register_add_command_id(reaper_str!("HB_SHOW_HIDE_PLAYTIME"))?;
        session.plugin_register_add_hook_command::<MyHookCommand>()?;
        session.plugin_register_add_gaccel(OwnedGaccelRegister::without_key_binding(
            my_command_id,
            "Show/hide Playtime",
        ))?;
        let extension = Self {
            my_command_id,
            reaper_session: Fragile::new(session),
        };
        Ok(extension)
    }
}

struct MyHookCommand;

impl HookCommand for MyHookCommand {
    fn call(command_id: CommandId, _flag: i32) -> bool {
        if command_id != extension().my_command_id {
            return false;
        }
        let plugin_context = extension()
            .reaper_session
            .get()
            .reaper()
            .low()
            .plugin_context();
        let pointers = HelgoboxApiPointers::load(&plugin_context);
        let session = HelgoboxApiSession::new(pointers);
        session.HB_ShowOrHidePlaytime(45);
        // println!("Executing my command: {res}!");
        true
    }
}
