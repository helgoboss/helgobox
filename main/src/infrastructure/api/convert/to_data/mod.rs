use crate::application;
use crate::domain::GroupId;
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema::{
    OscArgKind, VirtualControlElementId, VirtualControlElementKind,
};
use crate::infrastructure::data;
pub use compartment::*;
pub use mapping::*;
use source::*;
use std::str::FromStr;

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

fn convert_osc_arg_type(s: OscArgKind) -> helgoboss_learn::OscTypeTag {
    use helgoboss_learn::OscTypeTag as T;
    use OscArgKind::*;
    match s {
        Float => T::Float,
        Double => T::Double,
        Bool => T::Bool,
        Nil => T::Nil,
        Inf => T::Inf,
        Int => T::Int,
        String => T::String,
        Blob => T::Blob,
        Time => T::Time,
        Long => T::Long,
        Char => T::Char,
        Color => T::Color,
        Midi => T::Midi,
        Array => T::Array,
    }
}

fn convert_group_key(
    group_key: Option<String>,
    group_id_by_key: impl Fn(&str) -> Option<GroupId>,
) -> ConversionResult<GroupId> {
    let group_id = if let Some(key) = group_key {
        group_id_by_key(&key)
            .or_else(|| GroupId::from_str(&key).ok())
            .ok_or_else(|| format!("Group {} not defined", key))?
    } else {
        GroupId::default()
    };
    Ok(group_id)
}
