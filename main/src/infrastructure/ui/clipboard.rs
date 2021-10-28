use arboard::Clipboard;

pub fn copy_text_to_clipboard(text: String) {
    let mut clipboard = Clipboard::new().expect("couldn't create clipboard");
    clipboard
        .set_text(text)
        .expect("couldn't set clipboard contents");
}

pub fn get_text_from_clipboard() -> Option<String> {
    let mut clipboard = Clipboard::new().ok()?;
    clipboard.get_text().ok()
}
