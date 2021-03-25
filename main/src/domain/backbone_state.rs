use crate::domain::{
    ControlInput, DeviceControlInput, DeviceFeedbackOutput, FeedbackOutput, RealearnTargetContext,
    ReaperTarget,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

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
    upper_floor_instances: RefCell<HashSet<String>>,
}

impl BackboneState {
    pub fn new(target_context: RealearnTargetContext) -> Self {
        Self {
            target_context: RefCell::new(target_context),
            last_touched_target: Default::default(),
            control_input_usage: Default::default(),
            feedback_output_usage: Default::default(),
            upper_floor_instances: Default::default(),
        }
    }

    pub fn target_context() -> &'static RefCell<RealearnTargetContext> {
        &BackboneState::get().target_context
    }

    pub fn last_touched_target(&self) -> Option<ReaperTarget> {
        self.last_touched_target.borrow().clone()
    }

    pub fn add_to_upper_floor(&self, instance_id: String) {
        self.upper_floor_instances.borrow_mut().insert(instance_id);
    }

    pub fn remove_from_upper_floor(&self, instance_id: &str) {
        self.upper_floor_instances.borrow_mut().remove(instance_id);
    }

    pub fn control_is_allowed(&self, instance_id: &str, control_input: ControlInput) -> bool {
        if let Some(dev_input) = control_input.device_input() {
            self.interaction_is_allowed(instance_id, dev_input, &self.control_input_usage)
        } else {
            true
        }
    }

    pub fn feedback_is_allowed(&self, instance_id: &str, feedback_output: FeedbackOutput) -> bool {
        if let Some(dev_output) = feedback_output.device_output() {
            self.interaction_is_allowed(instance_id, dev_output, &self.feedback_output_usage)
        } else {
            true
        }
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

    fn interaction_is_allowed<D: Eq + Hash>(
        &self,
        instance_id: &str,
        device: D,
        usages: &RefCell<HashMap<D, HashSet<String>>>,
    ) -> bool {
        let upper_floor_instances = self.upper_floor_instances.borrow();
        if upper_floor_instances.is_empty() || upper_floor_instances.contains(instance_id) {
            // There's no instance living on a higher floor.
            true
        } else {
            // There's at least one instance living on a higher floor and it's not ours.
            let usages = usages.borrow();
            if let Some(instances) = usages.get(&device) {
                if instances.len() <= 1 {
                    // It's just us using this device (or nobody, but shouldn't happen).
                    true
                } else {
                    // Other instances use this device as well.
                    // Allow usage only if none of these instances are on the upper floor.
                    !instances
                        .iter()
                        .any(|id| upper_floor_instances.contains(id))
                }
            } else {
                // No instance using this device (shouldn't happen because at least we use it).
                true
            }
        }
    }
}
