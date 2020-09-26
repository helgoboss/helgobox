use reaper_high::Reaper;
use uuid::Uuid;

pub fn prompt_for(caption: &str) -> Option<String> {
    Reaper::get()
        .medium_reaper()
        .get_user_inputs("ReaLearn", 1, "Controller name", 256)
        .map(|r| r.into_string())
}
