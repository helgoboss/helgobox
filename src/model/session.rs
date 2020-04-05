use crate::model::MidiSourceModel;

#[derive(Default)]
pub struct RealearnSession {
    dummy_source_model: MidiSourceModel<'static>,
}

impl RealearnSession {
    pub fn new() -> RealearnSession {
        RealearnSession::default()
    }

    pub fn get_dummy_source_model(&self) -> &MidiSourceModel<'static> {
        &self.dummy_source_model
    }
}
