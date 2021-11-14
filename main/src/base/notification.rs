use once_cell::sync::Lazy;
use reaper_high::Reaper;
use reaper_medium::{MessageBoxType, ReaperStringArg};
use std::sync::Mutex;

pub fn notify_processing_result(heading: &str, msgs: Vec<String>) {
    let joined_msg = msgs.join("\n\n");
    let msg = format!(
        "{}\n{}\n\n{}\n\n",
        heading,
        "-".repeat(heading.len()),
        joined_msg
    );
    Reaper::get().show_console_msg(msg);
}

pub fn warn(msg: String) {
    static PREV_MSG: Lazy<Mutex<String>> = Lazy::new(Default::default);
    let mut prev_msg = PREV_MSG.lock().unwrap();
    let reaper = Reaper::get();
    if msg == *prev_msg {
        reaper.show_console_msg("|");
    } else {
        reaper.show_console_msg(format!("\n\nReaLearn warning: {} ", msg));
        *prev_msg = msg;
    }
}

pub fn alert<'a>(msg: impl Into<ReaperStringArg<'a>>) {
    Reaper::get()
        .medium_reaper()
        .show_message_box(msg, "ReaLearn", MessageBoxType::Okay);
}
