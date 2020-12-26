pub struct FileBasedPresetLinkManager {}

impl FileBasedPresetLinkManager {
    pub fn new() -> FileBasedPresetLinkManager {
        FileBasedPresetLinkManager {}
    }

    pub fn find_linked_fx(&self, preset_id: &str) -> Option<String> {
        // TODO-high implement correctly
        None
    }

    pub fn link_to_fx(&self, preset_id: &str, fx_name: &str) {
        // TODO-high implement correctly
    }

    pub fn unlink_from_fx(&self, preset_id: &str) {
        // TODO-high implement correctly
    }
}
