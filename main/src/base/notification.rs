use reaper_high::Reaper;
use reaper_medium::{MessageBoxType, ReaperStringArg};
use std::error::Error;

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

pub fn warn_user_on_anyhow_error(result: anyhow::Result<()>) {
    if let Err(e) = result {
        warn_user_about_anyhow_error(e)
    }
}

pub fn warn_user_about_anyhow_error(error: anyhow::Error) {
    warn(format!("{error:#}"));
}

pub fn warn(msg: String) {
    Reaper::get().show_console_msg(format!("\n\nReaLearn warning: {msg} "));
}

#[allow(dead_code)]
pub fn notify_user_on_error(result: Result<(), Box<dyn Error>>) {
    if let Err(e) = result {
        notify_user_about_error(e);
    }
}

#[allow(dead_code)]
pub fn notify_user_on_anyhow_error(result: anyhow::Result<()>) {
    if let Err(e) = result {
        notify_user_about_anyhow_error(&e);
    }
}

#[allow(dead_code)]
pub fn notify_user_about_error(e: Box<dyn Error>) {
    alert(e.to_string());
}

#[allow(dead_code)]
pub fn notify_user_about_anyhow_error(e: &anyhow::Error) {
    alert(format!("{e:#}"));
}

pub fn alert<'a>(msg: impl Into<ReaperStringArg<'a>>) {
    Reaper::get()
        .medium_reaper()
        .show_message_box(msg, "Helgobox", MessageBoxType::Okay);
}
