use once_cell::sync::Lazy;
use reaper_high::Reaper;
use reaper_medium::{MessageBoxType, ReaperStringArg};
use std::sync::Mutex;

pub fn warn(msg: String) {
    static PREV_MSG: Lazy<Mutex<String>> = Lazy::new(Default::default);
    let mut prev_msg = PREV_MSG.lock().unwrap();
    let reaper = Reaper::get();
    if msg == *prev_msg {
        reaper.show_console_msg("|");
    } else {
        reaper.show_console_msg(format!("\nReaLearn warning: {} ", msg));
        *prev_msg = msg;
    }
}

pub fn alert<'a>(msg: impl Into<ReaperStringArg<'a>>) {
    Reaper::get()
        .medium_reaper()
        .show_message_box(msg, "ReaLearn", MessageBoxType::Okay);
}
