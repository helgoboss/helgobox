use reaper_high::Reaper;
use reaper_medium::{MessageBoxType, ReaperStringArg};

pub fn warn(msg: &str) {
    Reaper::get().show_console_msg(format!("ReaLearn warning: {}\n", msg));
}

pub fn alert<'a>(msg: impl Into<ReaperStringArg<'a>>) {
    Reaper::get()
        .medium_reaper()
        .show_message_box(msg, "ReaLearn", MessageBoxType::Okay);
}
