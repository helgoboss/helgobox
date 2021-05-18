use reaper_high::Reaper;

/// Attention: This blocks the thread but continues the event loop, so you shouldn't have
/// anything borrowed while calling this unless you want errors due to reentrancy.
pub fn prompt_for(caption: &str, initial_value: &str) -> Option<String> {
    Reaper::get()
        .medium_reaper()
        .get_user_inputs("ReaLearn", 1, caption, initial_value, 256)
        .map(|r| r.to_str().trim().to_owned())
        .filter(|r| !r.is_empty())
}
