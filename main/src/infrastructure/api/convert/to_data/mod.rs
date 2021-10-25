use crate::application;
use crate::infrastructure::api::schema::{VirtualControlElementId, VirtualControlElementKind};
use crate::infrastructure::data;
pub use compartment::*;
pub use mapping::*;
use source::*;

mod compartment;
mod glue;
mod group;
mod mapping;
mod parameter;
mod source;
mod target;

fn convert_control_element_type(
    s: VirtualControlElementKind,
) -> application::VirtualControlElementType {
    use application::VirtualControlElementType as T;
    use VirtualControlElementKind::*;
    match s {
        Multi => T::Multi,
        Button => T::Button,
    }
}

fn convert_control_element_id(s: VirtualControlElementId) -> data::VirtualControlElementIdData {
    use data::VirtualControlElementIdData as T;
    use VirtualControlElementId::*;
    match s {
        Indexed(i) => T::Indexed(i),
        Named(s) => T::Named(s),
    }
}
