use crate::application::{ActivationType, ModifierConditionModel, ProgramConditionModel};
use crate::core::{prop, Prop};
use core::fmt;
use rx_util::UnitEvent;
use serde::{Deserialize, Serialize};
use smallvec::alloc::fmt::Formatter;
use std::cell::RefCell;
use std::rc::Rc;
use uuid::Uuid;

/// A mapping group.
#[derive(Debug)]
pub struct GroupModel {
    id: GroupId,
    pub name: Prop<String>,
    pub control_is_enabled: Prop<bool>,
    pub feedback_is_enabled: Prop<bool>,
    pub activation_type: Prop<ActivationType>,
    pub modifier_condition_1: Prop<ModifierConditionModel>,
    pub modifier_condition_2: Prop<ModifierConditionModel>,
    pub program_condition: Prop<ProgramConditionModel>,
    pub eel_condition: Prop<String>,
}

impl fmt::Display for GroupModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name.get_ref())
    }
}

/// See MappingModel for explanation.
impl PartialEq for GroupModel {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self as _, other as _)
    }
}

pub type SharedGroup = Rc<RefCell<GroupModel>>;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId {
    uuid: Uuid,
}

impl GroupId {
    pub fn random() -> GroupId {
        GroupId {
            uuid: Uuid::new_v4(),
        }
    }
}

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uuid)
    }
}

impl GroupModel {
    pub fn new(name: String) -> Self {
        Self {
            id: GroupId::random(),
            name: prop(name),
            control_is_enabled: prop(true),
            feedback_is_enabled: prop(true),
            activation_type: prop(ActivationType::Always),
            modifier_condition_1: Default::default(),
            modifier_condition_2: Default::default(),
            program_condition: Default::default(),
            eel_condition: Default::default(),
        }
    }

    pub fn id(&self) -> GroupId {
        self.id
    }

    /// Fires whenever a property has changed that doesn't have an effect on control/feedback
    /// processing.
    pub fn changed_non_processing_relevant(&self) -> impl UnitEvent {
        self.name.changed()
    }

    /// Fires whenever a property has changed that has an effect on control/feedback processing.
    pub fn changed_processing_relevant(&self) -> impl UnitEvent {
        self.control_is_enabled
            .changed()
            .merge(self.feedback_is_enabled.changed())
            .merge(self.activation_type.changed())
            .merge(self.modifier_condition_1.changed())
            .merge(self.modifier_condition_2.changed())
            .merge(self.eel_condition.changed())
            .merge(self.program_condition.changed())
    }
}
