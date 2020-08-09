use crate::application::{ModeModelData, SourceModelData, TargetModelData};
use crate::domain::{MappingModel, SessionContext};
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MappingModelData {
    name: String,
    source: SourceModelData,
    mode: ModeModelData,
    target: TargetModelData,
    control_is_enabled: bool,
    feedback_is_enabled: bool,
}

impl Default for MappingModelData {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            source: Default::default(),
            mode: Default::default(),
            target: Default::default(),
            control_is_enabled: true,
            feedback_is_enabled: true,
        }
    }
}

impl MappingModelData {
    pub fn from_model(model: &MappingModel, context: &SessionContext) -> MappingModelData {
        MappingModelData {
            name: model.name.get_ref().clone(),
            source: SourceModelData::from_model(&model.source_model),
            mode: ModeModelData::from_model(&model.mode_model),
            target: TargetModelData::from_model(&model.target_model, context),
            control_is_enabled: model.control_is_enabled.get(),
            feedback_is_enabled: model.feedback_is_enabled.get(),
        }
    }

    pub fn to_model(&self, context: &SessionContext) -> MappingModel {
        let mut model = MappingModel::default();
        self.apply_to_model(&mut model, context).unwrap();
        model
    }

    fn apply_to_model(
        &self,
        model: &mut MappingModel,
        context: &SessionContext,
    ) -> Result<(), &'static str> {
        model.name.set_without_notification(self.name.clone());
        self.source
            .apply_to_model(model.source_model.borrow_mut())
            .unwrap();
        self.mode
            .apply_to_model(model.mode_model.borrow_mut())
            .unwrap();
        self.target
            .apply_to_model(model.target_model.borrow_mut(), context)
            .unwrap();
        model
            .control_is_enabled
            .set_without_notification(self.control_is_enabled);
        model
            .feedback_is_enabled
            .set_without_notification(self.feedback_is_enabled);
        Ok(())
    }
}
