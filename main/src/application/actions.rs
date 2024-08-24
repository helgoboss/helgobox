use crate::base::notification;
use reaper_high::{Action, Reaper};
use reaper_medium::SectionId;

pub fn build_smart_command_name_from_action(action: &Action) -> Option<String> {
    match action.command_name() {
        // Built-in actions don't have a command name but a persistent command ID.
        // Use command ID as string.
        None => action.command_id().ok().map(|id| id.to_string()),
        // ReaScripts and custom actions have a command name as persistent identifier.
        Some(name) => Some(name.into_string()),
    }
}

pub fn build_action_from_smart_command_name(
    section_id: SectionId,
    smart_command_name: &str,
) -> Option<Action> {
    match smart_command_name.parse::<u32>() {
        // Could parse this as command ID integer. This is a built-in action.
        Ok(command_id_int) => match command_id_int.try_into() {
            Ok(command_id) => Some(
                Reaper::get()
                    .section_by_id(section_id)
                    .action_by_command_id(command_id),
            ),
            Err(_) => {
                notification::warn(format!("Invalid command ID {command_id_int}"));
                None
            }
        },
        // Couldn't parse this as integer. This is a ReaScript or custom action.
        Err(_) => Some(Reaper::get().action_by_command_name(smart_command_name)),
    }
}
