use base::{
    make_available_globally_in_main_thread_on_demand, NamedChannelSender, SenderToNormalThread,
};

use crate::domain::{
    AdditionalFeedbackEvent, ControlInput, DeviceControlInput, DeviceFeedbackOutput,
    FeedbackOutput, InstanceId, RealearnSourceState, RealearnTargetState, ReaperTarget,
    ReaperTargetType, SafeLua, SharedInstance, StreamDeckDeviceId, StreamDeckDeviceManager,
    StreamDeckSourceFeedbackValue, UnitId, WeakInstance,
};
#[allow(unused)]
use anyhow::{anyhow, Context};
use pot::{PotFavorites, PotFilterExcludes};

use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use cached::proc_macro::cached;
use camino::Utf8PathBuf;
use egui::epaint::ahash::HashSetExt;
use fragile::Fragile;
use helgoboss_learn::{RgbColor, UnitValue};
use helgobox_api::persistence::{
    StreamDeckButtonBackground, StreamDeckButtonForeground, TargetTouchCause,
};
use once_cell::sync::Lazy;
use reaper_high::{Fx, Reaper};
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::hash::Hash;
use std::rc::Rc;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use streamdeck::{Kind, StreamDeck};
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
    // TODO-high-playtime-refactoring Since the introduction of units, foreign matrices are not used in practice. Let's
    //  keep this for a while and remove.
    instances: RefCell<NonCryptoHashMap<InstanceId, WeakInstance>>,
    was_processing_keyboard_input: Cell<bool>,
    global_pot_filter_exclude_list: RefCell<PotFilterExcludes>,
    recently_focused_fx_container: Rc<RefCell<RecentlyFocusedFxContainer>>,
    stream_deck_device_manager: RefCell<StreamDeckDeviceManager>,
    stream_decks: RefCell<NonCryptoHashMap<StreamDeckDeviceId, StreamDeck>>,
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
            stream_deck_device_manager: Default::default(),
            stream_decks: Default::default(),
        }
    }

    /// Returns IDs of newly connected Stream Deck devices.
    pub fn detect_stream_deck_device_changes(&self) -> NonCryptoHashSet<StreamDeckDeviceId> {
        let devices_in_use = self.stream_deck_device_manager.borrow().devices_in_use();
        let actually_connected_devices: NonCryptoHashSet<_> =
            self.stream_decks.borrow().keys().copied().collect();
        if devices_in_use == actually_connected_devices {
            return NonCryptoHashSet::new();
        }
        self.connect_or_disconnect_stream_deck_devices(&devices_in_use)
    }

    pub fn register_stream_deck_usage(&self, unit_id: UnitId, device: Option<StreamDeckDeviceId>) {
        // Change device usage
        let mut manager = self.stream_deck_device_manager.borrow_mut();
        manager.register_device_usage(unit_id, device);
        let devices_in_use = manager.devices_in_use();
        // Update connections
        self.connect_or_disconnect_stream_deck_devices(&devices_in_use);
    }

    fn connect_or_disconnect_stream_deck_devices(
        &self,
        devices_in_use: &NonCryptoHashSet<StreamDeckDeviceId>,
    ) -> NonCryptoHashSet<StreamDeckDeviceId> {
        let mut decks = self.stream_decks.borrow_mut();
        // Disconnect from devices that are not in use anymore
        decks.retain(|id, _| devices_in_use.contains(id));
        // Connect to devices
        devices_in_use
            .iter()
            .filter_map(|dev_id| {
                if decks.contains_key(dev_id) {
                    return None;
                }
                match dev_id.connect() {
                    Ok(dev) => {
                        decks.insert(*dev_id, dev);
                        Some(*dev_id)
                    }
                    Err(e) => {
                        tracing::warn!(msg = "Couldn't connect to Stream Deck device", %e);
                        None
                    }
                }
            })
            .collect()
    }

    pub fn stream_decks_mut(&self) -> RefMut<NonCryptoHashMap<StreamDeckDeviceId, StreamDeck>> {
        self.stream_decks.borrow_mut()
    }

    pub fn send_stream_deck_feedback(
        &self,
        dev_id: StreamDeckDeviceId,
        value: StreamDeckSourceFeedbackValue,
    ) -> anyhow::Result<()> {
        use image::{DynamicImage, Pixel, Rgba, RgbaImage};
        const DEFAULT_BG_COLOR: RgbColor = RgbColor::BLACK;
        const DEFAULT_FG_COLOR: RgbColor = RgbColor::WHITE;
        #[cached(option = true)]
        fn load_image_for_stream_deck(path: String, width: u32, height: u32) -> Option<RgbaImage> {
            let path: Utf8PathBuf = path.into();
            let path = if path.is_relative() {
                Reaper::get().resource_path().join(path)
            } else {
                path
            };
            let image = image::open(path).ok()?;
            let image = image
                .resize_to_fill(width, height, image::imageops::FilterType::Lanczos3)
                .into();
            Some(image)
        }
        fn blit_with_alpha(target: &mut RgbaImage, overlay: &RgbaImage, alpha: Option<f32>) {
            // for (x, y, overlay_pixel) in overlay.enumerate_pixels() {
            //     let target_pixel = target.get_pixel_mut(x, y);
            //     let blended_pixel = Rgba([
            //         (overlay_pixel[0] as f32 * alpha + target_pixel[0] as f32 * (1.0 - alpha))
            //             as u8,
            //         (overlay_pixel[1] as f32 * alpha + target_pixel[1] as f32 * (1.0 - alpha))
            //             as u8,
            //         (overlay_pixel[2] as f32 * alpha + target_pixel[2] as f32 * (1.0 - alpha))
            //             as u8,
            //         255,
            //     ]);
            //     *target_pixel = blended_pixel;
            // }
            for (x, y, target_pixel) in target.enumerate_pixels_mut() {
                let mut overlay_pixel = *overlay.get_pixel(x, y);
                if let Some(alpha) = alpha {
                    overlay_pixel[3] = (alpha * 255.0).round() as u8;
                }
                target_pixel.blend(&overlay_pixel)
            }
        }
        use std::f32::consts::PI;

        /// Draws a knob with an indicator showing the given value (0% to 100%).
        /// The knob is centered within the specified width and height.
        /// - `width`: The width of the image.
        /// - `height`: The height of the image.
        /// - `value`: The value to display, from 0.0 to 100.0 (percentage).
        /// - `filename`: The filename to save the image as.
        fn draw_knob(img: &mut RgbaImage, width: u32, height: u32, value: f32) {
            let cx = width as f32 / 2.0;
            let cy = height as f32 / 2.0;
            // Circle
            let radius = cx.min(cy) - 10.0;
            let gray = 40;
            let knob_color = Rgba([gray, gray, gray, 100]);
            imageproc::drawing::draw_filled_circle_mut(
                img,
                (cx as _, cy as _),
                radius as _,
                knob_color,
            );
            // Indicator line
            let angle = value * 2.0 * PI - PI / 2.0; // Start at the top (12 o'clock)
            let line_length = radius - 5.0;
            let line_x = cx + line_length * angle.cos();
            let line_y = cy + line_length * angle.sin();
            let indicator_color = Rgba([255, 255, 255, 1]);
            draw_line(
                img,
                cx as i32,
                cy as i32,
                line_x as i32,
                line_y as i32,
                indicator_color,
            );
        }

        /// Draws a simple line between two points on the image using Bresenham's line algorithm.
        fn draw_line(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba<u8>) {
            imageproc::drawing::draw_antialiased_line_segment_mut(
                img,
                (x0, y0),
                (x1, y1),
                color,
                |i, _, _| i,
            );
        }

        let mut stream_decks = self.stream_decks.borrow_mut();
        let sd = stream_decks
            .get_mut(&dev_id)
            .context("stream deck not connected")?;
        let (width, height) = match sd.kind() {
            Kind::Xl => (96, 96),
            Kind::Original | Kind::OriginalV2 | Kind::Mini | Kind::Mk2 => (72, 72),
        };
        // Paint grounding (important for images with alpha channel)
        let bg_color = value.background_color.unwrap_or(DEFAULT_BG_COLOR);
        let mut bg_layer = RgbaImage::from_pixel(width, height, bg_color.into());
        // Paint background
        match value.button_design.background {
            StreamDeckButtonBackground::Color(_) => {}
            StreamDeckButtonBackground::Image(b) => {
                if let Some(bg_img) = load_image_for_stream_deck(b.path, width, height) {
                    blit_with_alpha(&mut bg_layer, &bg_img, None);
                }
            }
        };
        // Paint foreground
        let fg_color = value.foreground_color.unwrap_or(DEFAULT_FG_COLOR);
        if value.numeric_value.is_some() || value.text_value.is_some() {
            let fb_value = value.numeric_value.unwrap_or(UnitValue::MIN).get() as f32;
            match value.button_design.foreground {
                StreamDeckButtonForeground::FadingColor(_) => {
                    let mut fg_color: Rgba<u8> = fg_color.into();
                    fg_color[3] = (fb_value * 255.0).round() as u8;
                    for pixel in bg_layer.pixels_mut() {
                        pixel.blend(&fg_color);
                    }
                }
                StreamDeckButtonForeground::FadingImage(b) => {
                    let mut fg_layer = RgbaImage::from_pixel(width, height, fg_color.into());
                    if let Some(fg_img) = load_image_for_stream_deck(b.path, width, height) {
                        blit_with_alpha(&mut fg_layer, &fg_img, None);
                    }
                    blit_with_alpha(&mut bg_layer, &fg_layer, Some(fb_value));
                }
                StreamDeckButtonForeground::FullBar(_) => {
                    let mut fg_color: Rgba<u8> = fg_color.into();
                    fg_color[3] = 100;
                    let rect_height = (height as f32 * fb_value) as u32;
                    // Fill the background
                    for (_, y, pixel) in bg_layer.enumerate_pixels_mut() {
                        if y >= height - rect_height {
                            pixel.blend(&fg_color);
                        }
                    }
                }
                StreamDeckButtonForeground::Knob(_) => {
                    draw_knob(&mut bg_layer, width, height, fb_value);
                }
            }
        }
        sd.set_button_image(value.button_index as _, DynamicImage::ImageRgba8(bg_layer))?;
        Ok(())
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

    pub fn control_is_allowed(&self, unit_id: &UnitId, control_input: ControlInput) -> bool {
        if let Some(dev_input) = control_input.device_input() {
            self.interaction_is_allowed(unit_id, dev_input, &self.control_input_usages)
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

    /// Returns the last relevant focused FX even if it's not focused anymore and even if it's closed.
    ///
    /// Returns `None` only if no FX has been focused yet or if the last focused FX doesn't exist anymore.
    ///
    /// One special thing about this is that this doesn't necessarily return the currently focused
    /// FX. It could also be the previously focused one. That's important because when queried from ReaLearn UI, the
    /// current one is mostly ReaLearn itself - which is in most cases not what we want.
    pub fn last_relevant_available_focused_fx(&self, this_realearn_fx: &Fx) -> Option<Fx> {
        self.recently_focused_fx_container
            .borrow()
            .last_relevant_available_fx(this_realearn_fx)
            .cloned()
    }

    pub fn feedback_is_allowed(&self, unit_id: &UnitId, feedback_output: FeedbackOutput) -> bool {
        if let Some(dev_output) = feedback_output.device_output() {
            self.interaction_is_allowed(unit_id, dev_output, &self.feedback_output_usages)
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
    pub fn last_relevant_available_fx(&self, this_realearn_fx: &Fx) -> Option<&Fx> {
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
