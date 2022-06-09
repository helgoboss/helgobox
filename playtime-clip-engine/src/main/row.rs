use playtime_api::persistence as api;

#[derive(Clone, Debug)]
pub struct Row {}

impl Row {
    pub fn save(&self) -> api::Row {
        api::Row {
            name: None,
            tempo: None,
            time_signature: None,
        }
    }
}
