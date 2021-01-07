use crate::application::{
    ActivationType, MappingModel, ModifierConditionModel, ProgramConditionModel,
};
use crate::core::default_util::{bool_true, is_bool_true, is_default};
use crate::domain::{MappingCompartment, MappingId, ProcessorContext};
use crate::infrastructure::data::{ModeModelData, SourceModelData, TargetModelData};
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnabledData {
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    pub control_is_enabled: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    pub feedback_is_enabled: bool,
}
