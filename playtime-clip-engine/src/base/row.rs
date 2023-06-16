use playtime_api::persistence as api;
use playtime_api::persistence::RowId;

#[derive(Clone, Debug)]
pub struct Row {
    id: RowId,
    name: Option<String>,
}

impl Row {
    pub fn from_api_row(api_row: api::Row) -> Self {
        Self {
            id: api_row.id,
            name: api_row.name,
        }
    }

    pub fn new(id: RowId) -> Self {
        Self { id, name: None }
    }

    pub fn save(&self) -> api::Row {
        api::Row {
            id: self.id.clone(),
            name: self.name.clone(),
            tempo: None,
            time_signature: None,
        }
    }
}
