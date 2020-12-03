use clipboard::{ClipboardContext, ClipboardProvider};
use reaper_high::{resolve_symbols_from_text, ActionKind, Reaper};

pub fn register_resolve_symbols_action() {
    Reaper::get().register_action(
        "REALEARN_RESOLVE_SYMBOLS",
        "[developer] ReaLearn: Resolve symbols from clipboard",
        || {
            if let Err(e) = resolve_symbols_from_clipboard() {
                Reaper::get().show_console_msg(format!("{}\n", e.to_string()));
            }
        },
        ActionKind::NotToggleable,
    );
}

fn resolve_symbols_from_clipboard() -> Result<(), Box<dyn std::error::Error>> {
    let mut clipboard: ClipboardContext =
        ClipboardProvider::new().map_err(|_| "Couldn't obtain clipboard.")?;
    let text = clipboard
        .get_contents()
        .map_err(|_| "Couldn't read from clipboard.")?;
    resolve_symbols_from_text(&text)
}
