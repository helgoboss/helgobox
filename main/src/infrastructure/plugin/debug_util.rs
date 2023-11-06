use crate::infrastructure::ui::get_text_from_clipboard;
use reaper_high::{resolve_symbols_from_text, ActionKind, Reaper};

pub fn resolve_symbols_from_clipboard() -> Result<(), Box<dyn std::error::Error>> {
    let text = get_text_from_clipboard().ok_or("Couldn't read from clipboard.")?;
    resolve_symbols_from_text(&text)
}
