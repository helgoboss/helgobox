use crate::application::{
    ActivationConditionCommand, ActivationConditionModel, ActivationConditionProp, Affected,
    Change, GetProcessingRelevance, GroupData, ProcessingRelevance,
};
use crate::domain::{CompartmentKind, GroupId, GroupKey, Tag};
use core::fmt;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub enum GroupCommand {
    SetName(String),
    SetTags(Vec<Tag>),
    SetControlIsEnabled(bool),
    SetFeedbackIsEnabled(bool),
    ChangeActivationCondition(ActivationConditionCommand),
}

pub enum GroupProp {
    Name,
    Tags,
    ControlIsEnabled,
    FeedbackIsEnabled,
    InActivationCondition(Affected<ActivationConditionProp>),
}

impl GetProcessingRelevance for GroupProp {
    fn processing_relevance(&self) -> Option<ProcessingRelevance> {
        use GroupProp as P;
        match self {
            P::Tags | P::ControlIsEnabled | P::FeedbackIsEnabled => {
                Some(ProcessingRelevance::ProcessingRelevant)
            }
            P::InActivationCondition(p) => p.processing_relevance(),
            P::Name => None,
        }
    }
}

/// A mapping group.
#[derive(Clone, Debug)]
pub struct GroupModel {
    compartment: CompartmentKind,
    id: GroupId,
    key: GroupKey,
    name: String,
    tags: Vec<Tag>,
    control_is_enabled: bool,
    feedback_is_enabled: bool,
    pub activation_condition_model: ActivationConditionModel,
}

impl Change<'_> for GroupModel {
    type Command = GroupCommand;
    type Prop = GroupProp;

    fn change(&mut self, cmd: GroupCommand) -> Option<Affected<GroupProp>> {
        use Affected::*;
        use GroupCommand as C;
        use GroupProp as P;
        let affected = match cmd {
            C::SetName(v) => {
                self.name = v;
                One(P::Name)
            }
            C::SetTags(v) => {
                self.tags = v;
                One(P::Tags)
            }
            C::SetControlIsEnabled(v) => {
                self.control_is_enabled = v;
                One(P::ControlIsEnabled)
            }
            C::SetFeedbackIsEnabled(v) => {
                self.feedback_is_enabled = v;
                One(P::FeedbackIsEnabled)
            }
            C::ChangeActivationCondition(cmd) => {
                return self
                    .activation_condition_model
                    .change(cmd)
                    .map(|affected| One(P::InActivationCondition(affected)));
            }
        };
        Some(affected)
    }
}

impl GroupModel {
    pub fn effective_name(&self) -> &str {
        if self.is_default_group() {
            "<Default>"
        } else {
            self.name()
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn control_is_enabled(&self) -> bool {
        self.control_is_enabled
    }

    pub fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled
    }

    pub fn activation_condition_model(&self) -> &ActivationConditionModel {
        &self.activation_condition_model
    }
}

impl fmt::Display for GroupModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.effective_name())
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
    pub fn new_from_ui(compartment: CompartmentKind, name: String) -> Self {
        Self::new_internal(compartment, GroupId::random(), GroupKey::random(), name)
    }

    pub fn new_from_data(compartment: CompartmentKind, id: GroupId, key: GroupKey) -> Self {
        Self::new_internal(compartment, id, key, "".to_string())
    }

    pub fn default_for_compartment(compartment: CompartmentKind) -> Self {
        Self {
            compartment,
            id: GroupId::default(),
            key: GroupKey::default(),
            name: Default::default(),
            tags: Default::default(),
            control_is_enabled: true,
            feedback_is_enabled: true,
            activation_condition_model: ActivationConditionModel::default(),
        }
    }

    fn new_internal(
        compartment: CompartmentKind,
        id: GroupId,
        key: GroupKey,
        name: String,
    ) -> Self {
        Self {
            id,
            key,
            name,
            ..Self::default_for_compartment(compartment)
        }
    }

    pub fn compartment(&self) -> CompartmentKind {
        self.compartment
    }

    pub fn id(&self) -> GroupId {
        self.id
    }

    pub fn key(&self) -> &GroupKey {
        &self.key
    }

    pub fn is_default_group(&self) -> bool {
        self.id.is_default()
    }

    pub fn create_data(&self) -> GroupData {
        GroupData {
            control_is_enabled: self.control_is_enabled(),
            feedback_is_enabled: self.feedback_is_enabled(),
            activation_condition: self
                .activation_condition_model
                .create_activation_condition(),
            tags: self.tags.clone(),
        }
    }
}
