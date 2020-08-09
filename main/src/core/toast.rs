use reaper_high::Reaper;

pub fn warn(msg: &str) {
    Reaper::get().show_console_msg(format!("ReaLearn warning: {}\n", msg));
}
