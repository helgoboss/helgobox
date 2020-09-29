use reaper_high::Reaper;

pub fn prompt_for(_caption: &str) -> Option<String> {
    Reaper::get()
        .medium_reaper()
        .get_user_inputs("ReaLearn", 1, "Controller name", 256)
        .map(|r| r.into_string())
}
