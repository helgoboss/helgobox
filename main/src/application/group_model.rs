use crate::application::{ActivationConditionModel, GroupData};
use crate::base::{prop, Prop};
use crate::domain::{GroupId, MappingCompartment, Tag};
use core::fmt;
use rxrust::prelude::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// A mapping group.
#[derive(Clone, Debug)]
pub struct GroupModel {
    compartment: MappingCompartment,
    id: GroupId,
    pub name: Prop<String>,
    pub tags: Prop<Vec<Tag>>,
    pub control_is_enabled: Prop<bool>,
    pub feedback_is_enabled: Prop<bool>,
    pub activation_condition_model: ActivationConditionModel,
}

impl GroupModel {
    pub fn name(&self) -> &str {
        if self.is_default_group() {
            "<Default>"
        } else {
            self.name.get_ref()
        }
    }
}

impl fmt::Display for GroupModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
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

impl GroupModel {
    pub fn new_from_ui(compartment: MappingCompartment, name: String) -> Self {
        Self::new_internal(compartment, GroupId::random(), name)
    }

    pub fn new_from_data(compartment: MappingCompartment, id: GroupId) -> Self {
        Self::new_internal(compartment, id, "".to_string())
    }

    pub fn default_for_compartment(compartment: MappingCompartment) -> Self {
        Self {
            compartment,
            id: Default::default(),
            name: Default::default(),
            tags: Default::default(),
            control_is_enabled: prop(true),
            feedback_is_enabled: prop(true),
            activation_condition_model: ActivationConditionModel::default(),
        }
    }

    fn new_internal(compartment: MappingCompartment, id: GroupId, name: String) -> Self {
        Self {
            id,
            name: prop(name),
            ..Self::default_for_compartment(compartment)
        }
    }

    pub fn compartment(&self) -> MappingCompartment {
        self.compartment
    }

    pub fn id(&self) -> GroupId {
        self.id
    }

    pub fn is_default_group(&self) -> bool {
        self.id.is_default()
    }

    pub fn create_data(&self) -> GroupData {
        GroupData {
            control_is_enabled: self.control_is_enabled.get(),
            feedback_is_enabled: self.feedback_is_enabled.get(),
            activation_condition: self
                .activation_condition_model
                .create_activation_condition(),
            tags: self.tags.get_ref().clone(),
        }
    }

    /// Fires whenever a property has changed that doesn't have an effect on control/feedback
    /// processing.
    pub fn changed_non_processing_relevant(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.name.changed()
    }

    /// Fires whenever a property has changed that has an effect on control/feedback processing.
    pub fn changed_processing_relevant(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.control_is_enabled
            .changed()
            .merge(self.feedback_is_enabled.changed())
            .merge(self.tags.changed())
            .merge(
                self.activation_condition_model
                    .changed_processing_relevant(),
            )
    }
}
