use crate::application::{
    ActivationType, Affected, BankConditionModel, Change, GetProcessingRelevance,
    ModifierConditionModel, ProcessingRelevance,
};
use crate::domain::{
    ActivationCondition, EelCondition, ExpressionCondition, ExpressionEvaluator, MappingId,
};

#[allow(clippy::enum_variant_names)]
pub enum ActivationConditionCommand {
    SetActivationType(ActivationType),
    SetModifierCondition1(ModifierConditionModel),
    SetModifierCondition2(ModifierConditionModel),
    SetBankCondition(BankConditionModel),
    SetScript(String),
    SetMappingId(Option<MappingId>),
}

#[derive(Eq, PartialEq)]
pub enum ActivationConditionProp {
    ActivationType,
    ModifierCondition1,
    ModifierCondition2,
    BankCondition,
    Script,
    MappingId,
}

impl GetProcessingRelevance for ActivationConditionProp {
    fn processing_relevance(&self) -> Option<ProcessingRelevance> {
        Some(ProcessingRelevance::ProcessingRelevant)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ActivationConditionModel {
    activation_type: ActivationType,
    modifier_condition_1: ModifierConditionModel,
    modifier_condition_2: ModifierConditionModel,
    bank_condition: BankConditionModel,
    script: String,
    mapping_id: Option<MappingId>,
}

impl Change<'_> for ActivationConditionModel {
    type Command = ActivationConditionCommand;
    type Prop = ActivationConditionProp;

    fn change(
        &mut self,
        cmd: ActivationConditionCommand,
    ) -> Option<Affected<ActivationConditionProp>> {
        use ActivationConditionCommand as C;
        use ActivationConditionProp as P;
        use Affected::*;
        let affected = match cmd {
            C::SetActivationType(v) => {
                self.activation_type = v;
                One(P::ActivationType)
            }
            C::SetModifierCondition1(v) => {
                self.modifier_condition_1 = v;
                One(P::ModifierCondition1)
            }
            C::SetModifierCondition2(v) => {
                self.modifier_condition_2 = v;
                One(P::ModifierCondition2)
            }
            C::SetBankCondition(v) => {
                self.bank_condition = v;
                One(P::BankCondition)
            }
            C::SetScript(v) => {
                self.script = v;
                One(P::Script)
            }
            C::SetMappingId(v) => {
                self.mapping_id = v;
                One(P::MappingId)
            }
        };
        Some(affected)
    }
}

impl ActivationConditionModel {
    pub fn activation_type(&self) -> ActivationType {
        self.activation_type
    }

    pub fn modifier_condition_1(&self) -> ModifierConditionModel {
        self.modifier_condition_1
    }

    pub fn modifier_condition_2(&self) -> ModifierConditionModel {
        self.modifier_condition_2
    }

    pub fn bank_condition(&self) -> BankConditionModel {
        self.bank_condition
    }

    pub fn script(&self) -> &str {
        &self.script
    }

    pub fn mapping_id(&self) -> Option<MappingId> {
        self.mapping_id
    }

    pub fn create_activation_condition(&self) -> ActivationCondition {
        use ActivationType::*;
        match self.activation_type() {
            Always => ActivationCondition::Always,
            Modifiers => {
                let conditions = self
                    .modifier_conditions()
                    .filter_map(|m| m.create_modifier_condition())
                    .collect();
                ActivationCondition::Modifiers(conditions)
            }
            Bank => ActivationCondition::Program {
                param_index: self.bank_condition().param_index(),
                program_index: self.bank_condition().bank_index(),
            },
            Eel => match EelCondition::compile(self.script()) {
                Ok(c) => ActivationCondition::Eel(Box::new(c)),
                Err(_) => ActivationCondition::Always,
            },
            Expression => match ExpressionCondition::compile(self.script()) {
                Ok(e) => ActivationCondition::Expression(Box::new(e)),
                Err(_) => ActivationCondition::Always,
            },
            TargetValue => match ExpressionEvaluator::compile(self.script()) {
                Ok(e) => ActivationCondition::TargetValue {
                    lead_mapping: self.mapping_id,
                    condition: Box::new(e),
                },
                Err(_) => ActivationCondition::Always,
            },
        }
    }

    fn modifier_conditions(&self) -> impl Iterator<Item = ModifierConditionModel> {
        use std::iter::once;
        once(self.modifier_condition_1()).chain(once(self.modifier_condition_2()))
    }
}
