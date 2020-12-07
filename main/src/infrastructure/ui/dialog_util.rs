use reaper_high::Reaper;
use reaper_medium::{MessageBoxType, ReaperStringArg};

pub fn prompt_for(caption: &str, initial_value: &str) -> Option<String> {
    Reaper::get()
        .medium_reaper()
        .get_user_inputs("ReaLearn", 1, caption, initial_value, 256)
        .map(|r| r.into_string())
}

pub fn alert<'a>(msg: impl Into<ReaperStringArg<'a>>) {
    Reaper::get()
        .medium_reaper()
        .show_message_box(msg, "ReaLearn", MessageBoxType::Okay);
}
