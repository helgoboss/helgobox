mod source;
pub use source::*;
mod glue;
pub use glue::*;
mod mapping;
pub use mapping::*;
mod target;
use crate::application::VirtualControlElementType;
use crate::domain::{GroupId, Tag};
use crate::infrastructure::api::schema;
use crate::infrastructure::data::VirtualControlElementIdData;
use helgoboss_learn::OscTypeTag;
pub use target::*;

fn convert_control_element_id(v: VirtualControlElementIdData) -> schema::VirtualControlElementId {
    use schema::VirtualControlElementId as T;
    use VirtualControlElementIdData::*;
    match v {
        Indexed(i) => T::Indexed(i),
        Named(n) => T::Named(n),
    }
}

fn convert_control_element_kind(
    v: VirtualControlElementType,
) -> Option<schema::VirtualControlElementKind> {
    use schema::VirtualControlElementKind as T;
    use VirtualControlElementType::*;
    let res = match v {
        Multi => T::Multi,
        Button => T::Button,
    };
    Some(res)
}

fn convert_osc_argument(
    arg_index: Option<u32>,
    arg_type: OscTypeTag,
    arg_is_relative: bool,
) -> Option<schema::OscArgument> {
    let arg_index = arg_index?;
    let arg = schema::OscArgument {
        index: Some(arg_index),
        kind: Some(convert_osc_arg_kind(arg_type)),
        // TODO-high "relative" doesn't make sense for "Send OSC" target.
        relative: Some(arg_is_relative),
    };
    Some(arg)
}

fn convert_osc_arg_kind(v: OscTypeTag) -> schema::OscArgKind {
    use schema::OscArgKind as T;
    use OscTypeTag::*;
    match v {
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

fn convert_tags(tags: &Vec<Tag>) -> Option<Vec<String>> {
    let tags = tags.iter().map(|t| t.to_string()).collect();
    Some(tags)
}

fn convert_group_id(
    group_id: GroupId,
    group_key_by_id: impl Fn(GroupId) -> Option<String>,
) -> Option<String> {
    {
        if group_id.is_default() {
            None
        } else {
            let key = group_key_by_id(group_id).unwrap_or_else(|| group_id.to_string());
            Some(key)
        }
    }
}
