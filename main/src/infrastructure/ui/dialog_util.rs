use reaper_high::Reaper;

pub fn prompt_for(caption: &str, initial_value: &str) -> Option<String> {
    Reaper::get()
        .medium_reaper()
        .get_user_inputs("ReaLearn", 1, caption, initial_value, 256)
        .map(|r| r.into_string())
}
