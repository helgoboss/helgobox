use base::{
    make_available_globally_in_main_thread_on_demand, NamedChannelSender, SenderToNormalThread,
};

use crate::domain::{
    AdditionalFeedbackEvent, ControlInput, DeviceControlInput, DeviceFeedbackOutput,
    FeedbackOutput, InstanceId, RealearnSourceState, RealearnTargetState, ReaperTarget,
    ReaperTargetType, SafeLua, SharedInstance, UnitId, WeakInstance,
};
#[allow(unused)]
use anyhow::{anyhow, Context};
use pot::{PotFavorites, PotFilterExcludes};

use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use fragile::Fragile;
use helgobox_api::persistence::TargetTouchCause;
use once_cell::sync::Lazy;
use reaper_high::Fx;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::hash::Hash;
use std::rc::Rc;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use strum::EnumCount;

make_available_globally_in_main_thread_on_demand!(Backbone);

/// Just the old term as alias for easier class search.
type _BackboneState = Backbone;

/// This is the domain-layer "backbone" which can hold state that's shared among all ReaLearn
/// instances.
pub struct Backbone {
    time_of_start: Instant,
    additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    source_state: RefCell<RealearnSourceState>,
    target_state: RefCell<RealearnTargetState>,
    last_touched_targets_container: RefCell<LastTouchedTargetsContainer>,
    /// Value: Instance ID of the ReaLearn instance that owns the control input.
    control_input_usages: RefCell<NonCryptoHashMap<DeviceControlInput, NonCryptoHashSet<UnitId>>>,
    /// Value: Instance ID of the ReaLearn instance that owns the feedback output.
    feedback_output_usages:
        RefCell<NonCryptoHashMap<DeviceFeedbackOutput, NonCryptoHashSet<UnitId>>>,
    superior_units: RefCell<NonCryptoHashSet<UnitId>>,
    /// We hold pointers to all ReaLearn instances in order to let instance B
    /// borrow a clip matrix which is owned by instance A. This is great because it allows us to
    /// control the same clip matrix from different controllers.
    // TODO-high-playtime-refactoring Since the introduction of units, foreign matrixes are not used in practice. Let's
    //  keep this for a while and remove.
    instances: RefCell<NonCryptoHashMap<InstanceId, WeakInstance>>,
    was_processing_keyboard_input: Cell<bool>,
    global_pot_filter_exclude_list: RefCell<PotFilterExcludes>,
    recently_focused_fx_container: Rc<RefCell<RecentlyFocusedFxContainer>>,
}

#[derive(Debug, Default)]
pub struct AnyThreadBackboneState {
    /// Thread-safe because we need to access the favorites both from the main thread (e.g. for
    /// display purposes) and from the pot worker (for building the collections). Alternative would
    /// be to clone the favorites whenever we build the collections.
    pub pot_favorites: RwLock<PotFavorites>,
}

impl AnyThreadBackboneState {
    pub fn get() -> &'static AnyThreadBackboneState {
        static INSTANCE: Lazy<AnyThreadBackboneState> = Lazy::new(AnyThreadBackboneState::default);
        &INSTANCE
    }
}

struct LastTouchedTargetsContainer {
    /// Contains the most recently touched targets at the end!
    last_target_touches: Vec<TargetTouch>,
}

struct TargetTouch {
    pub target: ReaperTarget,
    pub caused_by_realearn: bool,
}

impl Default for LastTouchedTargetsContainer {
    fn default() -> Self {
        // Each target type can be there twice: Once touched via ReaLearn, once touched in other way
        let max_count = ReaperTargetType::COUNT * 2;
        Self {
            last_target_touches: Vec::with_capacity(max_count),
        }
    }
}

impl LastTouchedTargetsContainer {
    /// Returns `true` if the last touched target has changed.
    pub fn update(&mut self, event: TargetTouchEvent) -> bool {
        // Don't do anything if the given target is the same as the last touched one
        if let Some(last_target_touch) = self.last_target_touches.last() {
            if event.target == last_target_touch.target
                && event.caused_by_realearn == last_target_touch.caused_by_realearn
            {
                return false;
            }
        }
        // Remove all previous entries of that target type and conditions
        let last_touched_target_type = ReaperTargetType::from_target(&event.target);
        self.last_target_touches.retain(|t| {
            ReaperTargetType::from_target(&t.target) != last_touched_target_type
                || t.caused_by_realearn != event.caused_by_realearn
        });
        // Push it as last touched target
        let touch = TargetTouch {
            target: event.target,
            caused_by_realearn: event.caused_by_realearn,
        };
        self.last_target_touches.push(touch);
        true
    }

    pub fn find(&self, filter: LastTouchedTargetFilter) -> Option<&ReaperTarget> {
        let touch = self.last_target_touches.iter().rev().find(|t| {
            match filter.touch_cause {
                TargetTouchCause::Reaper if t.caused_by_realearn => return false,
                TargetTouchCause::Realearn if !t.caused_by_realearn => return false,
                _ => {}
            }
            let target_type = ReaperTargetType::from_target(&t.target);
            filter.included_target_types.contains(&target_type)
        })?;
        Some(&touch.target)
    }
}

pub struct LastTouchedTargetFilter<'a> {
    pub included_target_types: &'a NonCryptoHashSet<ReaperTargetType>,
    pub touch_cause: TargetTouchCause,
}

impl<'a> LastTouchedTargetFilter<'a> {
    pub fn matches(&self, event: &TargetTouchEvent) -> bool {
        // Check touch cause
        match self.touch_cause {
            TargetTouchCause::Realearn if !event.caused_by_realearn => return false,
            TargetTouchCause::Reaper if event.caused_by_realearn => return false,
            _ => {}
        }
        // Check target types
        let actual_target_type = ReaperTargetType::from_target(&event.target);
        self.included_target_types.contains(&actual_target_type)
    }
}

impl Backbone {
    pub fn new(
        additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
        target_context: RealearnTargetState,
    ) -> Self {
        Self {
            time_of_start: Instant::now(),
            additional_feedback_event_sender,
            source_state: Default::default(),
            target_state: RefCell::new(target_context),
            last_touched_targets_container: Default::default(),
            control_input_usages: Default::default(),
            feedback_output_usages: Default::default(),
            superior_units: Default::default(),
            instances: Default::default(),
            was_processing_keyboard_input: Default::default(),
            global_pot_filter_exclude_list: Default::default(),
            recently_focused_fx_container: Default::default(),
        }
    }

    pub fn duration_since_time_of_start(&self) -> Duration {
        self.time_of_start.elapsed()
    }

    pub fn pot_filter_exclude_list(&self) -> Ref<PotFilterExcludes> {
        self.global_pot_filter_exclude_list.borrow()
    }

    pub fn pot_filter_exclude_list_mut(&self) -> RefMut<PotFilterExcludes> {
        self.global_pot_filter_exclude_list.borrow_mut()
    }

    /// Sets a flag that indicates that there's at least one ReaLearn mapping (in any instance)
    /// which matched some computer keyboard input in this main loop cycle. This flag will be read
    /// and reset a bit later in the same main loop cycle by [`RealearnControlSurfaceMiddleware`].
    pub fn set_keyboard_input_match_flag(&self) {
        self.was_processing_keyboard_input.set(true);
    }

    /// Resets the flag which indicates that there was at least one ReaLearn mapping which matched
    /// some computer keyboard input. Returns whether the flag was set.
    pub fn reset_keyboard_input_match_flag(&self) -> bool {
        self.was_processing_keyboard_input.replace(false)
    }

    /// Returns a static reference to a Lua state, intended to be used in the main thread only!
    ///
    /// This should only be used for Lua stuff like MIDI scripts, where it would be too expensive
    /// to create a new Lua state for each single script and too complex to have narrow-scoped
    /// lifetimes. For all other situations, a new Lua state should be constructed.
    ///
    /// # Panics
    ///
    /// Panics if not called from main thread.
    ///
    /// # Safety
    ///
    /// If this static reference is passed to other user threads and used there, we are done.
    pub unsafe fn main_thread_lua() -> &'static SafeLua {
        static LUA: Lazy<Fragile<SafeLua>> = Lazy::new(|| Fragile::new(SafeLua::new().unwrap()));
        LUA.get()
    }

    pub fn source_state() -> &'static RefCell<RealearnSourceState> {
        &Backbone::get().source_state
    }

    pub fn target_state() -> &'static RefCell<RealearnTargetState> {
        &Backbone::get().target_state
    }

    /// Returns the last touched targets (max. one per touchable type, so not much more than a
    /// dozen). The most recently touched ones are at the end, so it's ascending order!
    pub fn extract_last_touched_targets(&self) -> Vec<ReaperTarget> {
        self.last_touched_targets_container
            .borrow()
            .last_target_touches
            .iter()
            .map(|t| t.target.clone())
            .collect()
    }

    pub fn find_last_touched_target(
        &self,
        filter: LastTouchedTargetFilter,
    ) -> Option<ReaperTarget> {
        let container = self.last_touched_targets_container.borrow();
        container.find(filter).cloned()
    }

    pub fn is_superior(&self, instance_id: &UnitId) -> bool {
        self.superior_units.borrow().contains(instance_id)
    }

    pub fn make_superior(&self, instance_id: UnitId) {
        self.superior_units.borrow_mut().insert(instance_id);
    }

    pub fn make_inferior(&self, instance_id: &UnitId) {
        self.superior_units.borrow_mut().remove(instance_id);
    }

    //
    // /// Returns and - if necessary - installs an owned clip matrix.
    // ///
    // /// If this instance already contains an owned clip matrix, returns it. If not, creates
    // /// and installs one, removing a possibly existing foreign matrix reference.
    // pub fn get_or_insert_owned_clip_matrix(&mut self) -> &mut playtime_clip_engine::base::Matrix {
    //     self.create_and_install_owned_clip_matrix_if_necessary();
    //     self.owned_clip_matrix_mut().unwrap()
    // }

    // TODO-high-playtime-refactoring Woah, ugly. This shouldn't be here anymore, the design involved and this dirt
    //  stayed. self is not used. Same with _mut.
    /// Grants immutable access to the clip matrix defined for the given ReaLearn instance,
    /// if one is defined.
    ///
    /// # Errors
    ///
    /// Returns an error if the given instance doesn't have any clip matrix defined.
    #[cfg(feature = "playtime")]
    pub fn with_clip_matrix<R>(
        &self,
        instance: &SharedInstance,
        f: impl FnOnce(&playtime_clip_engine::base::Matrix) -> R,
    ) -> anyhow::Result<R> {
        let instance = instance.borrow();
        let matrix = instance.get_playtime_matrix()?;
        Ok(f(matrix))
    }

    /// Grants mutable access to the clip matrix defined for the given ReaLearn instance,
    /// if one is defined.
    #[cfg(feature = "playtime")]
    pub fn with_clip_matrix_mut<R>(
        &self,
        instance: &SharedInstance,
        f: impl FnOnce(&mut playtime_clip_engine::base::Matrix) -> R,
    ) -> anyhow::Result<R> {
        let mut instance = instance.borrow_mut();
        let matrix = instance.get_playtime_matrix_mut()?;
        Ok(f(matrix))
    }

    pub fn register_instance(&self, id: InstanceId, instance: WeakInstance) {
        self.instances.borrow_mut().insert(id, instance);
    }

    pub(super) fn unregister_instance(&self, id: &InstanceId) {
        self.instances.borrow_mut().remove(id);
    }

    pub fn control_is_allowed(&self, instance_id: &UnitId, control_input: ControlInput) -> bool {
        if let Some(dev_input) = control_input.device_input() {
            self.interaction_is_allowed(instance_id, dev_input, &self.control_input_usages)
        } else {
            true
        }
    }

    #[allow(dead_code)]
    pub fn find_instance(&self, instance_id: InstanceId) -> Option<SharedInstance> {
        let weak_instance_states = self.instances.borrow();
        let weak_instance_state = weak_instance_states.get(&instance_id)?;
        weak_instance_state.upgrade()
    }

    /// This should be called whenever the focused FX changes.
    ///
    /// We use this in order to be able to access the previously focused FX at all times.
    pub fn notify_fx_focused(&self, new_fx: Option<Fx>) {
        self.recently_focused_fx_container.borrow_mut().feed(new_fx);
    }

    /// The special thing about this is that this doesn't necessarily return the currently focused
    /// FX. It could also be the previously focused one.
    ///
    /// That's important because when queried from ReaLearn UI, the current one
    /// is mostly ReaLearn itself - which is in most cases not what we want.
    pub fn last_relevant_focused_fx_id(&self, this_realearn_fx: &Fx) -> Option<Fx> {
        self.recently_focused_fx_container
            .borrow()
            .last_relevant_fx(this_realearn_fx)
            .cloned()
    }

    pub fn feedback_is_allowed(
        &self,
        instance_id: &UnitId,
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
        instance_id: &UnitId,
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

    pub(super) fn notify_target_touched(&self, event: TargetTouchEvent) {
        let has_changed = self
            .last_touched_targets_container
            .borrow_mut()
            .update(event);
        if has_changed {
            self.additional_feedback_event_sender
                .send_complaining(AdditionalFeedbackEvent::LastTouchedTargetChanged)
        }
    }

    fn interaction_is_allowed<D: Eq + Hash>(
        &self,
        instance_id: &UnitId,
        device: D,
        usages: &RefCell<NonCryptoHashMap<D, NonCryptoHashSet<UnitId>>>,
    ) -> bool {
        let superior_instances = self.superior_units.borrow();
        if superior_instances.is_empty() || superior_instances.contains(instance_id) {
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
                    !instances.iter().any(|id| superior_instances.contains(id))
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
    usages: &mut NonCryptoHashMap<D, NonCryptoHashSet<UnitId>>,
    instance_id: &UnitId,
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

#[derive(Clone, Debug)]
pub struct TargetTouchEvent {
    pub target: ReaperTarget,
    pub caused_by_realearn: bool,
}

#[derive(Debug, Default)]
struct RecentlyFocusedFxContainer {
    previous: Option<Fx>,
    current: Option<Fx>,
}

impl RecentlyFocusedFxContainer {
    pub fn last_relevant_fx(&self, this_realearn_fx: &Fx) -> Option<&Fx> {
        [self.current.as_ref(), self.previous.as_ref()]
            .into_iter()
            .flatten()
            .find(|fx| fx.is_available() && *fx != this_realearn_fx)
    }

    pub fn feed(&mut self, new_fx: Option<Fx>) {
        // Never clear any memorized FX.
        let Some(new_fx) = new_fx else {
            return;
        };
        // Don't rotate if current FX has not changed.
        if let Some(current) = self.current.as_ref() {
            if &new_fx == current {
                return;
            }
        }
        // Rotate
        self.previous = self.current.take();
        self.current = Some(new_fx);
    }
}
