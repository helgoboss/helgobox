use crate::model::MidiSourceModel;

#[derive(Default, Debug)]
pub struct RealearnSession<'a> {
    dummy_source_model: MidiSourceModel<'a>,
}

impl<'a> RealearnSession<'a> {
    pub fn new() -> RealearnSession<'a> {
        RealearnSession::default()
    }

    pub fn get_dummy_source_model(&mut self) -> &mut MidiSourceModel<'a> {
        &mut self.dummy_source_model
    }
}
