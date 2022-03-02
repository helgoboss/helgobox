use crate::domain::{
    ClipMatrixRef, ControlInput, DeviceControlInput, DeviceFeedbackOutput, FeedbackOutput,
    InstanceId, InstanceState, InstanceStateChanged, NormalAudioHookTask, NormalRealTimeTask,
    QualifiedClipMatrixEvent, RealTimeSender, RealearnClipMatrix, RealearnTargetContext,
    ReaperTarget, SharedInstanceState, WeakInstanceState,
};
use reaper_high::Track;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;

make_available_globally_in_main_thread_on_demand!(BackboneState);

/// This is the domain-layer "backbone" which can hold state that's shared among all ReaLearn
/// instances.
pub struct BackboneState {
    target_context: RefCell<RealearnTargetContext>,
    last_touched_target: RefCell<Option<ReaperTarget>>,
    /// Value: Instance ID of the ReaLearn instance that owns the control input.
    control_input_usages: RefCell<HashMap<DeviceControlInput, HashSet<InstanceId>>>,
    /// Value: Instance ID of the ReaLearn instance that owns the feedback output.
    feedback_output_usages: RefCell<HashMap<DeviceFeedbackOutput, HashSet<InstanceId>>>,
    upper_floor_instances: RefCell<HashSet<InstanceId>>,
    /// We hold pointers to the instance state of all ReaLearn instances in order to let instance B
    /// borrow a clip matrix which is owned by instance A. This is great because it allows us to
    /// control the same clip matrix from different controllers.
    instance_states: RefCell<HashMap<InstanceId, WeakInstanceState>>,
    server_event_sender: tokio::sync::broadcast::Sender<ServerEventType>,
}

pub type ServerEventType = f64;

impl BackboneState {
    pub fn new(target_context: RealearnTargetContext) -> Self {
        Self {
            target_context: RefCell::new(target_context),
            last_touched_target: Default::default(),
            control_input_usages: Default::default(),
            feedback_output_usages: Default::default(),
            upper_floor_instances: Default::default(),
            instance_states: Default::default(),
            server_event_sender: tokio::sync::broadcast::channel(1000).0,
        }
    }

    pub fn server_event_sender() -> &'static tokio::sync::broadcast::Sender<ServerEventType> {
        &BackboneState::get().server_event_sender
    }

    pub fn target_context() -> &'static RefCell<RealearnTargetContext> {
        &BackboneState::get().target_context
    }

    pub fn last_touched_target(&self) -> Option<ReaperTarget> {
        self.last_touched_target.borrow().clone()
    }

    pub fn lives_on_upper_floor(&self, instance_id: &InstanceId) -> bool {
        self.upper_floor_instances.borrow().contains(instance_id)
    }

    pub fn add_to_upper_floor(&self, instance_id: InstanceId) {
        self.upper_floor_instances.borrow_mut().insert(instance_id);
    }

    pub fn remove_from_upper_floor(&self, instance_id: &InstanceId) {
        self.upper_floor_instances.borrow_mut().remove(instance_id);
    }

    pub fn create_instance(
        &self,
        id: InstanceId,
        instance_feedback_event_sender: crossbeam_channel::Sender<InstanceStateChanged>,
        clip_matrix_event_sender: crossbeam_channel::Sender<QualifiedClipMatrixEvent>,
        audio_hook_task_sender: RealTimeSender<NormalAudioHookTask>,
        real_time_processor_sender: RealTimeSender<NormalRealTimeTask>,
        this_track: Option<Track>,
    ) -> SharedInstanceState {
        let instance_state = InstanceState::new(
            id,
            instance_feedback_event_sender,
            clip_matrix_event_sender,
            audio_hook_task_sender,
            real_time_processor_sender,
            this_track,
        );
        let shared_instance_state = Rc::new(RefCell::new(instance_state));
        self.instance_states
            .borrow_mut()
            .insert(id, Rc::downgrade(&shared_instance_state));
        shared_instance_state
    }

    /// Grants immutable access to the clip matrix defined for the given ReaLearn instance,
    /// if one is defined.
    ///
    /// In case the given ReaLearn instance is configured to borrow the clip matrix from another
    /// referenced instance, the provided matrix will be the one from that other instance.
    ///
    /// Provides `None` in the following cases:
    ///
    /// - The given instance doesn't have any clip matrix defined.
    /// - The referenced instance doesn't exist.
    /// - The referenced instance exists but has no clip matrix defined.   
    pub fn with_clip_matrix<R>(
        &self,
        instance_state: &SharedInstanceState,
        f: impl FnOnce(&RealearnClipMatrix) -> R,
    ) -> Result<R, &'static str> {
        use ClipMatrixRef::*;
        let other_instance_id = match instance_state
            .borrow()
            .clip_matrix_ref()
            .ok_or(NO_CLIP_MATRIX_SET)?
        {
            Owned(m) => return Ok(f(m)),
            BorrowedFromInstance(instance_id) => *instance_id,
        };
        let other_instance_state = self
            .instance_states
            .borrow()
            .get(&other_instance_id)
            .ok_or(REFERENCED_INSTANCE_NOT_AVAILABLE)?
            .upgrade()
            .ok_or(REFERENCED_INSTANCE_NOT_AVAILABLE)?;
        let other_instance_state = other_instance_state.borrow();
        match other_instance_state
            .clip_matrix_ref()
            .ok_or(REFERENCED_CLIP_MATRIX_NOT_AVAILABLE)?
        {
            Owned(m) => Ok(f(m)),
            BorrowedFromInstance(_) => Err(NESTED_CLIP_BORROW_NOT_SUPPORTED),
        }
    }

    /// Grants mutable access to the clip matrix defined for the given ReaLearn instance,
    /// if one is defined.
    pub fn with_clip_matrix_mut<R>(
        &self,
        instance_state: &SharedInstanceState,
        f: impl FnOnce(&mut RealearnClipMatrix) -> R,
    ) -> Result<R, &'static str> {
        use ClipMatrixRef::*;
        let other_instance_id = match instance_state
            .borrow_mut()
            .clip_matrix_ref_mut()
            .ok_or(NO_CLIP_MATRIX_SET)?
        {
            Owned(m) => return Ok(f(m)),
            BorrowedFromInstance(instance_id) => *instance_id,
        };
        let other_instance_state = self
            .instance_states
            .borrow()
            .get(&other_instance_id)
            .ok_or(REFERENCED_INSTANCE_NOT_AVAILABLE)?
            .upgrade()
            .ok_or(REFERENCED_INSTANCE_NOT_AVAILABLE)?;
        let mut other_instance_state = other_instance_state.borrow_mut();
        match other_instance_state
            .clip_matrix_ref_mut()
            .ok_or(REFERENCED_CLIP_MATRIX_NOT_AVAILABLE)?
        {
            Owned(m) => Ok(f(m)),
            BorrowedFromInstance(_) => Err(NESTED_CLIP_BORROW_NOT_SUPPORTED),
        }
    }

    pub(super) fn unregister_instance_state(&self, id: &InstanceId) {
        self.instance_states.borrow_mut().remove(id);
    }

    pub fn control_is_allowed(
        &self,
        instance_id: &InstanceId,
        control_input: ControlInput,
    ) -> bool {
        if let Some(dev_input) = control_input.device_input() {
            self.interaction_is_allowed(instance_id, dev_input, &self.control_input_usages)
        } else {
            true
        }
    }

    pub fn feedback_is_allowed(
        &self,
        instance_id: &InstanceId,
        feedback_output: FeedbackOutput,
    ) -> bool {
        if let Some(dev_output) = feedback_output.device_output() {
            self.interaction_is_allowed(instance_id, dev_output, &self.feedback_output_usages)
        } else {
            true
        }
    }

    /// Also drops all previous usage  of that instance.
    ///
    /// Returns true if this actually caused a change in *feedback output* usage.
    pub fn update_io_usage(
        &self,
        instance_id: &InstanceId,
        control_input: Option<DeviceControlInput>,
        feedback_output: Option<DeviceFeedbackOutput>,
    ) -> bool {
        {
            let mut usages = self.control_input_usages.borrow_mut();
            update_io_usage(&mut usages, instance_id, control_input);
        }
        {
            let mut usages = self.feedback_output_usages.borrow_mut();
            update_io_usage(&mut usages, instance_id, feedback_output)
        }
    }

    pub(super) fn set_last_touched_target(&self, target: ReaperTarget) {
        *self.last_touched_target.borrow_mut() = Some(target);
    }

    fn interaction_is_allowed<D: Eq + Hash>(
        &self,
        instance_id: &InstanceId,
        device: D,
        usages: &RefCell<HashMap<D, HashSet<InstanceId>>>,
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

/// Returns `true` if there was an actual change.
fn update_io_usage<D: Eq + Hash + Copy>(
    usages: &mut HashMap<D, HashSet<InstanceId>>,
    instance_id: &InstanceId,
    device: Option<D>,
) -> bool {
    let mut previously_used_device: Option<D> = None;
    for (dev, ids) in usages.iter_mut() {
        let was_removed = ids.remove(instance_id);
        if was_removed {
            previously_used_device = Some(*dev);
        }
    }
    if let Some(dev) = device {
        usages
            .entry(dev)
            .or_default()
            .insert(instance_id.to_owned());
    }
    device != previously_used_device
}

const NO_CLIP_MATRIX_SET: &str = "no clip matrix set for this instance";
const REFERENCED_INSTANCE_NOT_AVAILABLE: &str = "other instance not available";
const REFERENCED_CLIP_MATRIX_NOT_AVAILABLE: &str = "clip matrix of other instance not available";
const NESTED_CLIP_BORROW_NOT_SUPPORTED: &str = "clip matrix of other instance also borrows";
