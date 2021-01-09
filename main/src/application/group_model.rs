use crate::application::{ActivationConditionModel, GroupData};
use crate::core::{prop, Prop};
use core::fmt;
use rx_util::UnitEvent;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use uuid::Uuid;

/// A mapping group.
#[derive(Clone, Debug)]
pub struct GroupModel {
    id: GroupId,
    pub name: Prop<String>,
    pub control_is_enabled: Prop<bool>,
    pub feedback_is_enabled: Prop<bool>,
    pub activation_condition_model: ActivationConditionModel,
}

impl Default for GroupModel {
    fn default() -> Self {
        Self {
            id: Default::default(),
            name: Default::default(),
            control_is_enabled: prop(true),
            feedback_is_enabled: prop(true),
            activation_condition_model: ActivationConditionModel::default(),
        }
    }
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
pub type WeakGroup = Weak<RefCell<GroupModel>>;

pub fn share_group(group: GroupModel) -> SharedGroup {
    Rc::new(RefCell::new(group))
}

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct GroupId {
    uuid: Uuid,
}

impl GroupId {
    pub fn is_default(&self) -> bool {
        self.uuid.is_nil()
    }

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
    pub fn new_from_ui(name: String) -> Self {
        Self::new_internal(GroupId::random(), name)
    }

    pub fn new_from_data(id: GroupId) -> Self {
        Self::new_internal(id, "".to_string())
    }

    fn new_internal(id: GroupId, name: String) -> Self {
        Self {
            id,
            name: prop(name),
            ..Default::default()
        }
    }

    pub fn id(&self) -> GroupId {
        self.id
    }

    pub fn is_default_group(&self) -> bool {
        self.id() == Default::default()
    }

    pub fn create_data(&self) -> GroupData {
        GroupData {
            control_is_enabled: self.control_is_enabled.get(),
            feedback_is_enabled: self.feedback_is_enabled.get(),
            activation_condition: self
                .activation_condition_model
                .create_activation_condition(),
        }
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
            .merge(
                self.activation_condition_model
                    .changed_processing_relevant(),
            )
    }
}
