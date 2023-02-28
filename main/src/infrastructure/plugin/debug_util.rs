use crate::infrastructure::ui::get_text_from_clipboard;
use reaper_high::{resolve_symbols_from_text, ActionKind, Reaper};

pub fn register_resolve_symbols_action() {
    Reaper::get().register_action(
        "REALEARN_RESOLVE_SYMBOLS",
        "[developer] ReaLearn: Resolve symbols from clipboard",
        || {
            if let Err(e) = resolve_symbols_from_clipboard() {
                Reaper::get().show_console_msg(format!("{e}\n"));
            }
        },
        ActionKind::NotToggleable,
    );
}

fn resolve_symbols_from_clipboard() -> Result<(), Box<dyn std::error::Error>> {
    let text = get_text_from_clipboard().ok_or("Couldn't read from clipboard.")?;
    resolve_symbols_from_text(&text)
}
