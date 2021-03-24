use crate::domain::{
    ControlInput, DeviceControlInput, DeviceFeedbackOutput, FeedbackOutput, RealearnTargetContext,
    ReaperTarget,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

make_available_globally_in_main_thread_on_demand!(BackboneState);

/// This is the domain-layer "backbone" which can hold state that's shared among all ReaLearn
/// instances.
pub struct BackboneState {
    target_context: RefCell<RealearnTargetContext>,
    last_touched_target: RefCell<Option<ReaperTarget>>,
    /// Value: Instance ID of the ReaLearn instance that owns the control input.
    control_input_usage: RefCell<HashMap<DeviceControlInput, HashSet<String>>>,
    /// Value: Instance ID of the ReaLearn instance that owns the feedback output.
    feedback_output_usage: RefCell<HashMap<DeviceFeedbackOutput, HashSet<String>>>,
}

impl BackboneState {
    pub fn new(target_context: RealearnTargetContext) -> Self {
        Self {
            target_context: RefCell::new(target_context),
            last_touched_target: Default::default(),
            control_input_usage: Default::default(),
            feedback_output_usage: Default::default(),
        }
    }

    pub fn target_context() -> &'static RefCell<RealearnTargetContext> {
        &BackboneState::get().target_context
    }

    pub fn last_touched_target(&self) -> Option<ReaperTarget> {
        self.last_touched_target.borrow().clone()
    }

    /// Also drops all previous usage  of that instance.
    pub fn update_io_usage(
        &self,
        instance_id: String,
        control_input: Option<DeviceControlInput>,
        feedback_output: Option<DeviceFeedbackOutput>,
    ) {
        self.release_exclusive_access(&instance_id);
        if let Some(i) = control_input {
            self.control_input_usage
                .borrow_mut()
                .entry(i)
                .or_default()
                .insert(instance_id.clone());
        }
        if let Some(o) = feedback_output {
            self.feedback_output_usage
                .borrow_mut()
                .entry(o)
                .or_default()
                .insert(instance_id.clone());
        }
        println!(
            "TODO-high Control input usage: {:?}",
            self.control_input_usage
        );
        println!(
            "TODO-high Feedback output usage: {:?}",
            self.feedback_output_usage
        );
    }

    fn release_exclusive_access(&self, instance_id: &str) {
        for ids in self.control_input_usage.borrow_mut().values_mut() {
            ids.remove(instance_id);
        }
        for ids in self.feedback_output_usage.borrow_mut().values_mut() {
            ids.remove(instance_id);
        }
    }

    pub(super) fn set_last_touched_target(&self, target: ReaperTarget) {
        *self.last_touched_target.borrow_mut() = Some(target);
    }
}
