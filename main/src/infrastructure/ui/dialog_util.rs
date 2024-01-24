use crate::application::SharedUnitModel;
use crate::domain::{CompartmentKind, GroupId};
use reaper_high::Reaper;

/// Attention: This blocks the thread but continues the event loop, so you shouldn't have
/// anything borrowed while calling this unless you want errors due to reentrancy.
pub fn prompt_for(caption: &str, initial_value: &str) -> Option<String> {
    let captions_csv = format!("{caption},separator=|,extrawidth=200");
    Reaper::get()
        .medium_reaper()
        .get_user_inputs("ReaLearn", 1, captions_csv, initial_value, 256)
        .map(|r| r.to_str().trim().to_owned())
}

pub fn add_group_via_dialog(
    session: SharedUnitModel,
    compartment: CompartmentKind,
) -> Result<GroupId, &'static str> {
    if let Some(name) = prompt_for("Group name", "") {
        if name.trim().is_empty() {
            return Err("empty group name");
        }
        let id = session
            .borrow_mut()
            .add_group_with_default_values(compartment, name);
        Ok(id)
    } else {
        Err("cancelled")
    }
}
