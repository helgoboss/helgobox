use arboard::Clipboard;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static CLIPBOARD: Lazy<Mutex<Clipboard>> =
    Lazy::new(|| Mutex::new(Clipboard::new().expect("couldn't create clipboard")));

pub fn copy_text_to_clipboard(text: String) {
    let mut clipboard = CLIPBOARD.lock().unwrap();
    clipboard
        .set_text(text)
        .expect("couldn't set clipboard contents");
}

pub fn get_text_from_clipboard() -> Option<String> {
    let mut clipboard = CLIPBOARD.lock().unwrap();
    clipboard.get_text().ok()
}
