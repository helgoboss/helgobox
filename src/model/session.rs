use crate::model::MidiSourceModel;

// TODO Maybe make the session static. What's the point of having a lifetime parameter? We use it
//  always as static anyway and that is unnecessary code.
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
