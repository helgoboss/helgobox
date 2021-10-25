mod source;
pub use source::*;
mod glue;
pub use glue::*;
mod mapping;
pub use mapping::*;
mod target;
use crate::application::VirtualControlElementType;
use crate::infrastructure::api::schema;
use crate::infrastructure::data::VirtualControlElementIdData;
pub use target::*;

fn convert_control_element_id(v: VirtualControlElementIdData) -> schema::VirtualControlElementId {
    use schema::VirtualControlElementId as T;
    use VirtualControlElementIdData::*;
    match v {
        Indexed(i) => T::Indexed(i),
        Named(n) => T::Named(n),
    }
}

fn convert_control_element_kind(v: VirtualControlElementType) -> schema::VirtualControlElementKind {
    use schema::VirtualControlElementKind as T;
    use VirtualControlElementType::*;
    match v {
        Multi => T::Multi,
        Button => T::Button,
    }
}
