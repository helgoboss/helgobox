use crate::domain::{
    Backbone, ControlEvent, ControlEventTimestamp, DeviceControlInput, DeviceDiff,
    DeviceFeedbackOutput, DomainEventHandler, FeedbackOutput, FinalSourceFeedbackValue, InstanceId,
    MainProcessor, MidiDeviceChangeDetector, MidiDeviceChangePayload,
    MonitoringFxChainChangeDetector, OscDeviceId, OscInputDevice, OscScanResult,
    QualifiedInstanceEvent, ReaperConfigChangeDetector, ReaperMessage, ReaperTarget,
    SharedInstance, SharedMainProcessors, StreamDeckDevicePayload, TargetTouchEvent,
    TouchedTrackParameterType, UnitEvent, UnitId, WeakInstance,
};
use base::{metrics_util, Global, NamedChannelSender, SenderToNormalThread};
use crossbeam_channel::Receiver;
use reaper_high::{
    ChangeDetectionMiddleware, ChangeEvent, ControlSurfaceEvent, ControlSurfaceMiddleware,
    FutureMiddleware, Fx, FxParameter, MainTaskMiddleware, Project, Reaper,
};
use reaper_rx::ControlSurfaceRxMiddleware;
use rosc::{OscMessage, OscPacket};
use std::cell::RefCell;

use base::hash_util::{NonCryptoHashMap, NonCryptoIndexMap};
use base::metrics_util::measure_time;
use itertools::{EitherOrBoth, Itertools};
use reaper_medium::{
    CommandId, ExtSupportsExtendedTouchArgs, GetFocusedFx2Result, GetTouchStateArgs,
    GetTouchedOrFocusedFxCurrentlyFocusedFxResult, MediaTrack, MidiInputDeviceId,
    MidiOutputDeviceId, PositionInSeconds, ReaProject, ReaperNormalizedFxParamValue,
    SectionContext,
};
use rxrust::prelude::*;
use std::fmt::Debug;
use std::mem;
use tracing::debug;

type OscCaptureSender = async_channel::Sender<OscScanResult>;
type TargetCaptureSender = async_channel::Sender<TargetTouchEvent>;

const CONTROL_SURFACE_MAIN_TASK_BULK_SIZE: usize = 10;
const INSTANCE_EVENT_BULK_SIZE: usize = 30;
const ADDITIONAL_FEEDBACK_EVENT_BULK_SIZE: usize = 30;
const INSTANCE_ORCHESTRATION_EVENT_BULK_SIZE: usize = 30;
const OSC_INCOMING_BULK_SIZE: usize = 1000;

#[derive(Debug)]
pub struct RealearnControlSurfaceMiddleware<EH: DomainEventHandler> {
    change_detection_middleware: ChangeDetectionMiddleware,
    change_event_queue: RefCell<Vec<ChangeEvent>>,
    monitoring_fx_chain_change_detector: MonitoringFxChainChangeDetector,
    rx_middleware: ControlSurfaceRxMiddleware,
    instances: NonCryptoIndexMap<InstanceId, WeakInstance>,
    main_processors: SharedMainProcessors<EH>,
    main_task_receiver: Receiver<RealearnControlSurfaceMainTask<EH>>,
    instance_event_receiver: Receiver<QualifiedInstanceEvent>,
    #[cfg(feature = "playtime")]
    playtime: PlaytimeMiddleware,
    additional_feedback_event_receiver: Receiver<AdditionalFeedbackEvent>,
    instance_orchestration_event_receiver: Receiver<UnitOrchestrationEvent>,
    main_task_middleware: MainTaskMiddleware,
    future_middleware: FutureMiddleware,
    counter: u64,
    full_beats: NonCryptoHashMap<ReaProject, u32>,
    deprecated_fx_focus_state: Option<GetFocusedFx2Result>,
    _modern_fx_focus_state: Option<GetTouchedOrFocusedFxCurrentlyFocusedFxResult>,
    target_capture_senders: NonCryptoHashMap<Option<UnitId>, TargetCaptureSender>,
    osc_capture_sender: Option<OscCaptureSender>,
    osc_input_devices: Vec<OscInputDevice>,
    device_change_detector: MidiDeviceChangeDetector,
    reaper_config_change_detector: ReaperConfigChangeDetector,
    control_surface_event_sender: SenderToNormalThread<ControlSurfaceEvent<'static>>,
    control_surface_event_receiver: crossbeam_channel::Receiver<ControlSurfaceEvent<'static>>,
    #[cfg(debug_assertions)]
    last_undesired_allocation_count: u32,
    event_handler: Box<dyn ControlSurfaceEventHandler>,
    osc_buffer: Vec<OscPacket>,
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
struct PlaytimeMiddleware {
    clip_matrix_event_receiver: Receiver<crate::domain::QualifiedClipMatrixEvent>,
}

pub trait ControlSurfaceEventHandler: Debug {
    fn midi_input_devices_changed(
        &self,
        diff: &DeviceDiff<MidiInputDeviceId>,
        device_config_changed: bool,
    );
    fn midi_output_devices_changed(
        &self,
        diff: &DeviceDiff<MidiOutputDeviceId>,
        device_config_changed: bool,
    );

    fn process_reaper_change_events(&self, change_events: &[ChangeEvent]);
}

pub enum RealearnControlSurfaceMainTask<EH: DomainEventHandler> {
    AddInstance(InstanceId, WeakInstance),
    AddMainProcessor(MainProcessor<EH>),
    RemoveMainProcessor(UnitId),
    StartCapturingTargets(Option<UnitId>, TargetCaptureSender),
    StopCapturingTargets(Option<UnitId>),
    StartCapturingOsc(OscCaptureSender),
    StopCapturingOsc,
    SendAllFeedback,
}

/// Not all events in REAPER are communicated via a control surface, e.g. action invocations.
#[derive(Debug)]
pub enum AdditionalFeedbackEvent {
    ActionInvoked(ActionInvokedEvent),
    FxSnapshotLoaded(FxSnapshotLoadedEvent),
    /// Work around REAPER's inability to notify about parameter changes in
    /// monitoring FX by simulating the notification ourselves.
    /// Then parameter learning and feedback works at least for
    /// ReaLearn monitoring FX instances, which is especially
    /// useful for conditional activation.
    RealearnMonitoringFxParameterValueChanged(RealearnMonitoringFxParameterValueChangedEvent),
    ParameterAutomationTouchStateChanged(ParameterAutomationTouchStateChangedEvent),
    /// Beat-changed events are emitted only when the project is playing.
    ///
    /// We shouldn't change that because targets such as "Marker/region: Go to" or "Project: Seek"
    /// depend on this (see https://github.com/helgoboss/helgobox/issues/663).
    BeatChanged(BeatChangedEvent),
    MappedFxParametersChanged,
    /// This event is raised when an FX window loses focus and a non-FX window gains focus,
    /// and vice versa (not covered by FxFocused change event).
    ///
    /// Attention: This will only be fired for REAPER 7+!
    FocusSwitchedBetweenMainAndFx,
    /// Forwarded unit state event
    ///
    /// Not all unit events are forwarded, only those that might matter for other
    /// units.
    Unit {
        unit_id: UnitId,
        unit_event: UnitEvent,
    },
    LastTouchedTargetChanged,
}

#[derive(Debug)]
pub enum UnitOrchestrationEvent {
    /// Sent by a ReaLearn instance X if it releases control over a source.
    ///
    /// This enables other instances to take over control of that source before X finally "switches
    /// off lights".
    SourceReleased(SourceReleasedEvent),
    /// Whenever something about instance's device usage changes (either input or output or both
    /// potentially change).
    IoUpdated(IoUpdatedEvent),
}

/// Communicates changes in which input and output device a ReaLearn instance uses or used.
#[derive(Debug)]
pub struct IoUpdatedEvent {
    pub unit_id: UnitId,
    pub control_input: Option<DeviceControlInput>,
    pub control_input_used: bool,
    pub feedback_output: Option<DeviceFeedbackOutput>,
    pub feedback_output_used: bool,
    pub feedback_output_usage_might_have_changed: bool,
}

#[derive(Debug)]
pub struct SourceReleasedEvent {
    pub unit_id: UnitId,
    pub feedback_output: Option<FeedbackOutput>,
    pub feedback_value: FinalSourceFeedbackValue,
}

#[derive(Debug)]
pub struct BeatChangedEvent {
    pub project: Project,
    pub new_value: PositionInSeconds,
}

#[derive(Debug)]
pub struct ActionInvokedEvent {
    pub section_context: SectionContext<'static>,
    pub command_id: CommandId,
}

#[derive(Debug)]
pub struct FxSnapshotLoadedEvent {
    pub fx: Fx,
}

#[derive(Debug)]
pub struct RealearnMonitoringFxParameterValueChangedEvent {
    pub parameter: FxParameter,
    pub new_value: ReaperNormalizedFxParamValue,
}

#[derive(Debug)]
pub struct ParameterAutomationTouchStateChangedEvent {
    pub track: MediaTrack,
    pub parameter_type: TouchedTrackParameterType,
    pub new_value: bool,
}

impl<EH: DomainEventHandler> RealearnControlSurfaceMiddleware<EH> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        main_task_receiver: Receiver<RealearnControlSurfaceMainTask<EH>>,
        instance_event_receiver: Receiver<QualifiedInstanceEvent>,
        #[cfg(feature = "playtime")] clip_matrix_event_receiver: Receiver<
            crate::domain::QualifiedClipMatrixEvent,
        >,
        additional_feedback_event_receiver: Receiver<AdditionalFeedbackEvent>,
        instance_orchestration_event_receiver: Receiver<UnitOrchestrationEvent>,
        main_processors: SharedMainProcessors<EH>,
        event_handler: Box<dyn ControlSurfaceEventHandler>,
    ) -> Self {
        let mut device_change_detector = MidiDeviceChangeDetector::new();
        // Prevent change messages to be sent on load by polling one time and ignoring result.
        device_change_detector.poll_for_midi_input_device_changes();
        device_change_detector.poll_for_midi_output_device_changes();
        let (control_surface_event_sender, control_surface_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("control surface events");
        Self {
            change_detection_middleware: ChangeDetectionMiddleware::new(),
            change_event_queue: RefCell::new(Vec::with_capacity(100)),
            monitoring_fx_chain_change_detector: Default::default(),
            rx_middleware: ControlSurfaceRxMiddleware::new(Global::control_surface_rx().clone()),
            instances: Default::default(),
            main_processors,
            main_task_receiver,
            instance_event_receiver,
            #[cfg(feature = "playtime")]
            playtime: PlaytimeMiddleware {
                clip_matrix_event_receiver,
            },
            additional_feedback_event_receiver,
            instance_orchestration_event_receiver,
            main_task_middleware: Global::get().create_task_support_middleware(),
            future_middleware: Global::get().create_future_support_middleware(),
            counter: 0,
            full_beats: Default::default(),
            deprecated_fx_focus_state: Default::default(),
            _modern_fx_focus_state: Default::default(),
            target_capture_senders: Default::default(),
            osc_capture_sender: None,
            osc_input_devices: vec![],
            device_change_detector,
            reaper_config_change_detector: Default::default(),
            control_surface_event_sender,
            control_surface_event_receiver,
            #[cfg(debug_assertions)]
            last_undesired_allocation_count: 0,
            event_handler,
            osc_buffer: Default::default(),
        }
    }

    fn run_internal(&mut self) {
        let timestamp = ControlEventTimestamp::from_main_thread();
        #[cfg(debug_assertions)]
        {
            // TODO-medium-playtime-refactoring This is propagated using main processors but it's a global event. We
            //  should use the global control surface event handler for this! Or ShutdownDetectionPanel? After all,
            //  this is not needed in the domain! But not urgent. It's enabled in dev builds only.
            let current_undesired_allocation_count =
                helgobox_allocator::undesired_allocation_count();
            if current_undesired_allocation_count != self.last_undesired_allocation_count {
                self.last_undesired_allocation_count = current_undesired_allocation_count;
                let event = &crate::domain::InternalInfoEvent::UndesiredAllocationCountChanged;
                for p in self.main_processors.borrow_mut().iter_mut() {
                    p.process_info_event(event);
                }
            }
        }
        // Poll for change events that don't support notification. At the moment we execute this
        // only once per run() invocation to save resources. As a consequence, we can't detect
        // whether the changes were caused by ReaLearn targets or direct interaction with REAPER
        // (e.g. via mouse). However, since change events detected by polling are currently not
        // used for target learning / last touched target detection anyway, we don't need to know
        // about their cause.
        self.poll_for_more_change_events();
        // Process REAPER events that occurred since the last call of run(). All events
        // accumulated up to this point are most likely *not* caused by ReaLearn, but by direct
        // interaction with REAPER, e.g. changing track volume with the mouse.
        // However, there's one exception: At least one ReaLearn instance might have input set to
        // "Computer keyboard" and therefore process key strokes. In this case, the keyboard
        // processing chain might have run before and caused ReaLearn to invoke some target.
        // In this case, the backbone knows about it.
        let realearn_was_matching_keyboard_input =
            Backbone::get().reset_keyboard_input_match_flag();
        self.process_events(realearn_was_matching_keyboard_input);
        // Execute operations scheduled by ReaLearn (in different ways)
        self.main_task_middleware.run();
        self.future_middleware.run();
        self.rx_middleware.run();
        self.process_main_tasks();
        self.process_instance_orchestration_events();
        // Inform ReaLearn about various changes that are not relevant for target learning
        self.detect_reaper_config_changes();
        self.emit_focus_switch_between_main_and_fx_as_feedback_event();
        self.emit_instance_events();
        self.emit_stream_deck_events(timestamp);
        self.emit_beats_as_feedback_events();
        self.detect_device_changes(timestamp);
        self.process_incoming_osc_messages(timestamp);
        // Drive Playtime
        #[cfg(feature = "playtime")]
        {
            self.poll_playtime();
            self.process_incoming_clip_matrix_events();
        }
        // Finally let the ReaLearn main processors do their regular job (the instances)
        self.run_main_processors(timestamp);
        // Some control surface events might have been picked up during this call of run() while
        // the change event queue was borrowed. Process them now (most likely they are turned into
        // change events).
        self.process_deferred_control_surface_events();
        // Process REAPER events that were accumulated within this call of run(). All events
        // accumulated up to this point are caused by ReaLearn itself.
        self.process_events(true);
        self.counter += 1;
    }

    pub fn remove_instance(&mut self, id: InstanceId) {
        self.instances.remove(&id);
    }

    pub fn remove_main_processor(&mut self, id: UnitId) -> anyhow::Result<()> {
        self.main_processors
            .try_borrow_mut()?
            .retain(|p| p.unit_id() != id);
        Ok(())
    }

    pub fn set_osc_input_devices(&mut self, devs: Vec<OscInputDevice>) {
        self.osc_input_devices = devs;
    }

    pub fn shutdown(&self) {
        for m in &mut *self.main_processors.borrow_mut() {
            m.switch_lights_off();
        }
    }

    pub fn clear_osc_input_devices(&mut self) {
        self.osc_input_devices.clear();
    }

    /// Called when waking up ReaLearn (first instance appears again or the first time).
    pub fn wake_up(&self) {
        let mut change_events = vec![];
        self.change_detection_middleware.reset(|e| {
            change_events.push(e);
        });
        for i in self.instances() {
            i.borrow_mut()
                .process_control_surface_change_events(&change_events);
        }
        for m in &*self.main_processors.borrow() {
            m.process_control_surface_change_events(&change_events);
        }
        for e in change_events {
            self.rx_middleware.handle_change(e);
        }
        // We don't want to execute tasks which accumulated during the "downtime" of Reaper.
        // So we just consume all without executing them.
        self.main_task_middleware.reset();
        self.future_middleware.reset();
    }

    fn process_events(&mut self, caused_by_realearn: bool) {
        self.process_change_events(caused_by_realearn);
        self.process_incoming_additional_feedback(caused_by_realearn);
    }

    fn process_change_events(&mut self, caused_by_realearn: bool) {
        // Our goal is to always keep using the same change event queue vector so that we don't
        // have to reallocate on each main loop cycle. That's also why we use `drain` further down.
        // Previously, we just directly used the `change_event_queue` variable. However, an
        // issue occurred: https://github.com/helgoboss/helgobox/issues/672#issuecomment-1246997201.
        // A user got a panic while the main processors processed the change events ... which is not
        // optimal but can happen in certain cases that are a topic for another day, see the
        // comment. The issue we need to deal with here is another one: The same panic occurred
        // until eternity because at the next main loop cycle the same event was still in the queue!
        // That's exactly the reason why we now temporarily replace `change_event_queue` with an
        // empty vector (doesn't require allocation) and replace it again with the old vector
        // when everything run through successfully. To my knowledge, this is the only place where
        // we need this logic because everywhere else we use channels, which have this logic baked
        // in already ("pop instead of peek").
        // Long story short: The following line ensures we make a "pop" instead of a "peek" in order
        // to avoid an infinite loop in case of a panic.
        let mut normal_events = self.change_event_queue.replace(vec![]);
        // Important because we deferred the change event handling, so it could be invalid now!
        // See https://github.com/helgoboss/helgobox/issues/672.
        normal_events.retain(|e| e.is_still_valid());
        let monitoring_fx_events = metrics_util::measure_time(
            "helgobox.control_surface.detect_monitoring_fx_changes",
            || self.monitoring_fx_chain_change_detector.poll_for_changes(),
        );
        if normal_events.is_empty() && monitoring_fx_events.is_empty() {
            return;
        }
        self.event_handler
            .process_reaper_change_events(&normal_events);
        for i in self.instances() {
            i.borrow_mut()
                .process_control_surface_change_events(&normal_events);
        }
        // This is for feedback processing. No Rx!
        let main_processors = self.main_processors.borrow();
        for p in main_processors.iter() {
            p.process_control_surface_change_events(&normal_events);
            p.process_control_surface_change_events(&monitoring_fx_events);
        }
        // The rest is only for upper layers (e.g. UI), not for processing.
        for e in normal_events
            .drain(..)
            .chain(monitoring_fx_events.into_iter())
        {
            self.rx_middleware.handle_change(e.clone());
            if let Some(target) = ReaperTarget::touched_from_change_event(e) {
                process_touched_target(target, caused_by_realearn, &self.target_capture_senders);
            }
        }
        // Now that everything ran successfully, we can assign the old drained vector back to the
        // original event queue. This ensures we can keep using the previous memory allocation.
        *self.change_event_queue.borrow_mut() = normal_events;
    }

    fn process_deferred_control_surface_events(&self) {
        while let Ok(event) = self.control_surface_event_receiver.try_recv() {
            let mut change_event_queue = self.change_event_queue.borrow_mut();
            self.handle_event_internal(&event, &mut change_event_queue);
        }
    }

    fn process_main_tasks(&mut self) {
        for t in self
            .main_task_receiver
            .try_iter()
            .take(CONTROL_SURFACE_MAIN_TASK_BULK_SIZE)
        {
            use RealearnControlSurfaceMainTask::*;
            match t {
                AddInstance(id, instance) => {
                    self.instances.insert(id, instance);
                }
                AddMainProcessor(p) => {
                    self.main_processors.borrow_mut().push(p);
                }
                RemoveMainProcessor(unit_id) => {
                    self.main_processors
                        .try_borrow_mut()
                        .expect("asynchronous removal of main processor also failed")
                        .retain(|p| p.unit_id() != unit_id);
                }
                StartCapturingTargets(instance_id, sender) => {
                    self.target_capture_senders.insert(instance_id, sender);
                }
                StopCapturingTargets(instance_id) => {
                    self.target_capture_senders.remove(&instance_id);
                }
                StartCapturingOsc(sender) => {
                    self.osc_capture_sender = Some(sender);
                }
                StopCapturingOsc => {
                    self.osc_capture_sender = None;
                }
                SendAllFeedback => {
                    for m in &*self.main_processors.borrow() {
                        m.send_all_feedback();
                    }
                }
            }
        }
    }

    #[cfg(feature = "playtime")]
    fn poll_playtime(&mut self) {
        // Poll Playtime engine
        playtime_clip_engine::PlaytimeEngine::get().poll();
        // Poll all Playtime matrixes
        for instance in self.instances() {
            let (instance_id, events) = {
                let mut instance = instance.borrow_mut();
                (instance.id(), instance.poll_owned_clip_matrix())
            };
            for processor in &*self.main_processors.borrow() {
                processor.process_polled_clip_matrix_events(instance_id, &events);
            }
        }
    }

    fn instances(&self) -> impl Iterator<Item = SharedInstance> + '_ {
        self.instances.values().filter_map(|i| i.upgrade())
    }

    fn run_main_processors(&mut self, timestamp: ControlEventTimestamp) {
        for p in &mut *self.main_processors.borrow_mut() {
            p.run_essential(timestamp);
            p.run_control(timestamp);
        }
    }

    fn process_incoming_additional_feedback(&mut self, caused_by_realearn: bool) {
        for event in self
            .additional_feedback_event_receiver
            .try_iter()
            .take(ADDITIONAL_FEEDBACK_EVENT_BULK_SIZE)
        {
            if let AdditionalFeedbackEvent::RealearnMonitoringFxParameterValueChanged(e) = &event {
                let rx = Global::control_surface_rx();
                rx.fx_parameter_value_changed
                    .borrow_mut()
                    .next(e.parameter.clone());
                rx.fx_parameter_touched
                    .borrow_mut()
                    .next(e.parameter.clone());
            }
            for p in &mut *self.main_processors.borrow_mut() {
                p.process_additional_feedback_event(&event)
            }
            if let Some(target) = ReaperTarget::touched_from_additional_event(&event) {
                process_touched_target(target, caused_by_realearn, &self.target_capture_senders);
            }
        }
    }

    #[cfg(feature = "playtime")]
    fn process_incoming_clip_matrix_events(&mut self) {
        for event in self.playtime.clip_matrix_event_receiver.try_iter().take(30) {
            for i in self.instances() {
                i.borrow().process_non_polled_clip_matrix_event(&event);
            }
            for p in &mut *self.main_processors.borrow_mut() {
                p.process_non_polled_clip_matrix_event(&event);
            }
        }
    }

    fn process_instance_orchestration_events(&mut self) {
        for event in self
            .instance_orchestration_event_receiver
            .try_iter()
            .take(INSTANCE_ORCHESTRATION_EVENT_BULK_SIZE)
        {
            use UnitOrchestrationEvent::*;
            match event {
                SourceReleased(e) => {
                    debug!("Source of unit {} released", e.unit_id);
                    // We also allow the instance to take over which released the source in
                    // the first place! Simply because in the meanwhile, this instance
                    // could have found a new usage for it! E.g. likely to happen with
                    // preset changes.
                    let other_instance_took_over = self
                        .main_processors
                        .borrow()
                        .iter()
                        .any(|p| p.maybe_takeover_source(&e));
                    if !other_instance_took_over {
                        if let Some(p) = self
                            .main_processors
                            .borrow()
                            .iter()
                            .find(|p| p.unit_id() == e.unit_id)
                        {
                            // Finally safe to switch off lights!
                            p.finally_switch_off_source(e.feedback_output, e.feedback_value);
                        }
                    }
                }
                IoUpdated(e) => {
                    let backbone_state = Backbone::get();
                    let feedback_dev_usage_changed = backbone_state.update_io_usage(
                        &e.unit_id,
                        if e.control_input_used {
                            e.control_input
                        } else {
                            None
                        },
                        if e.feedback_output_used {
                            e.feedback_output
                        } else {
                            None
                        },
                    );
                    if feedback_dev_usage_changed && backbone_state.is_superior(&e.unit_id) {
                        debug!(
                            "Superior unit {} {} feedback output",
                            e.unit_id,
                            if e.feedback_output_used {
                                "claimed"
                            } else {
                                "released"
                            }
                        );
                        if let Some(feedback_output) = e.feedback_output {
                            // Give inferior instances the chance to cancel or reactivate.
                            self.main_processors
                                .borrow()
                                .iter()
                                .filter(|p| p.unit_id() != e.unit_id)
                                .for_each(|p| {
                                    p.handle_change_of_some_upper_floor_instance(feedback_output)
                                });
                        }
                    }
                }
            }
        }
    }

    fn detect_reaper_config_changes(&mut self) {
        let changes = self.reaper_config_change_detector.poll_for_changes();
        for p in &*self.main_processors.borrow() {
            p.process_reaper_config_changes(&changes);
        }
    }

    fn emit_focus_switch_between_main_and_fx_as_feedback_event(&mut self) {
        let pointers = Reaper::get().medium_reaper().low().pointers();
        // TODO The modern GetTouchedOrFocusedFX way needs more testing. I tried to use it hoping it would fix
        //  https://github.com/helgoboss/helgobox/issues/1367 but it essentially behaves the same
        //  as GetFocusedFX2, so there's no urge to migrate.
        // let fire = if pointers.GetTouchedOrFocusedFX.is_some() {
        //     // The latest and greatest way to detect focused FX
        //     self.detect_focus_switch_between_main_and_fx_as_feedback_event_modern()
        // } else
        let fire = if pointers.GetFocusedFX2.is_some() {
            // The deprecated way to detect focused FX
            self.detect_focus_switch_between_main_and_fx_as_feedback_event_deprecated()
        } else {
            // Detection of unfocusing FX (without focusing a new one) not supported in REAPER versions < 7
            false
        };
        if fire {
            let event = AdditionalFeedbackEvent::FocusSwitchedBetweenMainAndFx;
            for p in &mut *self.main_processors.borrow_mut() {
                p.process_additional_feedback_event(&event);
            }
        }
    }

    #[allow(unused)]
    fn detect_focus_switch_between_main_and_fx_as_feedback_event_modern(&mut self) -> bool {
        let reaper = Reaper::get().medium_reaper();
        let new = reaper.get_touched_or_focused_fx_currently_focused_fx();
        let last = mem::replace(&mut self._modern_fx_focus_state, new);
        match (last, new) {
            (None, None) => {
                // This happens continuously before any FX is focused
                false
            }
            (None, Some(_)) => {
                // The first time an FX is focused
                true
            }
            (Some(_), None) => {
                // Shouldn't happen because REAPER usually doesn't clear the last-focused FX. But if it happens,
                // we should fire.
                true
            }
            (Some(last), Some(new)) => {
                // Only fire if we changed from "no FX focused" to "FX focused" or vice versa (not when focusing
                // between FX ... this is detected better by REAPER's built-in control surface FxFocused event,
                // mainly because the latter doesn't fire when an FX is removed - which would be too much)
                last.is_still_focused != new.is_still_focused
            }
        }
    }

    #[allow(deprecated)]
    fn detect_focus_switch_between_main_and_fx_as_feedback_event_deprecated(&mut self) -> bool {
        let reaper = Reaper::get().medium_reaper();
        let new = reaper.get_focused_fx_2();
        let last = mem::replace(&mut self.deprecated_fx_focus_state, new);
        match (last, new) {
            (None, None) => {
                // This happens continuously before any FX is focused
                false
            }
            (None, Some(_)) => {
                // The first time an FX is focused
                true
            }
            (Some(_), None) => {
                // Shouldn't happen because REAPER usually doesn't clear the last-focused FX. But if it happens,
                // we should fire.
                true
            }
            (Some(last), Some(new)) => {
                // Only fire if we changed from "no FX focused" to "FX focused" or vice versa (not when focusing
                // between FX ... this is detected better by REAPER's built-in control surface FxFocused event,
                // mainly because the latter doesn't fire when an FX is removed - which would be too much)
                last.is_still_focused != new.is_still_focused
            }
        }
    }

    fn emit_instance_events(&mut self) {
        for event in self
            .instance_event_receiver
            .try_iter()
            .take(INSTANCE_EVENT_BULK_SIZE)
        {
            for p in &mut *self.main_processors.borrow_mut() {
                p.process_instance_event_for_feedback(&event)
            }
        }
    }

    fn emit_stream_deck_events(&mut self, timestamp: ControlEventTimestamp) {
        let backbone = Backbone::get();
        for msg in backbone.poll_stream_deck_messages() {
            for p in &mut *self.main_processors.borrow_mut() {
                if !p.wants_stream_deck_input_from(msg.dev_id) {
                    continue;
                }
                let event = ControlEvent::new(msg.msg, timestamp);
                p.process_incoming_stream_deck_msg(event);
            }
        }
    }

    fn emit_beats_as_feedback_events(&mut self) {
        for project in Reaper::get().projects() {
            let reference_pos = if project.is_playing() {
                project.play_position_latency_compensated()
            } else {
                project.edit_cursor_position().unwrap_or_default()
            };
            if self.record_possible_beat_change(project, reference_pos) {
                let event = AdditionalFeedbackEvent::BeatChanged(BeatChangedEvent {
                    project,
                    new_value: reference_pos,
                });
                for p in &mut *self.main_processors.borrow_mut() {
                    p.process_additional_feedback_event(&event);
                }
            }
        }
    }

    fn detect_device_changes(&mut self, timestamp: ControlEventTimestamp) {
        // Check roughly every 2 seconds
        if self.counter % (30 * 2) != 0 {
            return;
        }
        // Stream deck
        let added_stream_deck_device_ids = Backbone::get().detect_stream_deck_device_changes();
        // MIDI
        let midi_in_diff = self
            .device_change_detector
            .poll_for_midi_input_device_changes();
        let midi_out_diff = self
            .device_change_detector
            .poll_for_midi_output_device_changes();
        // Resetting MIDI devices is necessary especially on Windows.
        reset_midi_devices(
            midi_in_diff.added_devices.iter().copied(),
            midi_out_diff.added_devices.iter().copied(),
        );
        // Pass events to event handler
        if midi_in_diff.devices_changed() || midi_in_diff.device_config_changed {
            self.event_handler
                .midi_input_devices_changed(&midi_in_diff, midi_in_diff.device_config_changed);
        }
        if midi_out_diff.devices_changed() || midi_out_diff.device_config_changed {
            self.event_handler
                .midi_output_devices_changed(&midi_out_diff, midi_out_diff.device_config_changed);
        }
        // Emit as REAPER source messages
        let mut msg = Vec::with_capacity(2);
        if !midi_in_diff.added_devices.is_empty() || !midi_out_diff.added_devices.is_empty() {
            let payload = MidiDeviceChangePayload {
                input_devices: midi_in_diff.added_devices,
                output_devices: midi_out_diff.added_devices,
            };
            msg.push(ReaperMessage::MidiDevicesConnected(payload));
        }
        if !midi_in_diff.removed_devices.is_empty() || !midi_out_diff.removed_devices.is_empty() {
            let payload = MidiDeviceChangePayload {
                input_devices: midi_in_diff.removed_devices,
                output_devices: midi_out_diff.removed_devices,
            };
            msg.push(ReaperMessage::MidiDevicesDisconnected(payload));
        }
        if !added_stream_deck_device_ids.is_empty() {
            msg.push(ReaperMessage::StreamDeckDevicesConnected(
                StreamDeckDevicePayload {
                    devices: added_stream_deck_device_ids,
                },
            ));
        }
        // Inform main processors
        for p in &mut *self.main_processors.borrow_mut() {
            for msg in &msg {
                let evt = ControlEvent::new(msg, timestamp);
                p.process_reaper_message(evt);
            }
        }
    }

    fn process_incoming_osc_messages(&mut self, timestamp: ControlEventTimestamp) {
        for dev in &mut self.osc_input_devices {
            self.osc_buffer.clear();
            self.osc_buffer
                .extend(dev.poll_multiple(OSC_INCOMING_BULK_SIZE));
            for proc in &mut *self.main_processors.borrow_mut() {
                if proc.wants_osc_from(dev.id()) {
                    for packet in &self.osc_buffer {
                        let evt = ControlEvent::new(packet, timestamp);
                        proc.process_incoming_osc_packet(evt);
                    }
                }
            }
            if let Some(sender) = &self.osc_capture_sender {
                for packet in self.osc_buffer.drain(..) {
                    process_incoming_osc_packet_for_learning(*dev.id(), sender, packet)
                }
            }
        }
    }

    fn handle_event_internal(
        &self,
        event: &ControlSurfaceEvent,
        change_event_queue: &mut Vec<ChangeEvent>,
    ) -> bool {
        // We always need to forward to the change detection middleware even if we are in
        // a mode in which the detected change event doesn't matter!
        self.change_detection_middleware.process(event, |e| {
            // Notify backbone whenever focused FX changes
            if let ChangeEvent::FxFocused(evt) = &e {
                Backbone::get().notify_fx_focused(evt.fx.clone());
            }
            // We don't process change events immediately in order to be able to process
            // multiple events occurring in one main loop cycle as a natural batch. This
            // is important for performance reasons
            // (see https://github.com/helgoboss/helgobox/issues/553).
            change_event_queue.push(e);
        })
    }

    fn record_possible_beat_change(
        &mut self,
        project: Project,
        reference_pos: PositionInSeconds,
    ) -> bool {
        let beat_info = project.beat_info_at(reference_pos);
        let new_full_beats = beat_info.full_beats.get() as _;
        let full_beats = self.full_beats.entry(project.raw()).or_default();
        let beat_changed = new_full_beats != *full_beats;
        *full_beats = new_full_beats;
        beat_changed
    }

    fn poll_for_more_change_events(&mut self) {
        let mut change_event_queue = self.change_event_queue.borrow_mut();
        measure_time(
            "helgobox.control_surface.poll_for_more_change_events",
            || {
                self.change_detection_middleware.run(&mut |change_event| {
                    change_event_queue.push(change_event);
                });
            },
        );
    }
}

impl<EH: DomainEventHandler> ControlSurfaceMiddleware for RealearnControlSurfaceMiddleware<EH> {
    fn run(&mut self) {
        // Already-borrowed / reentrancy check
        if self.main_processors.try_borrow_mut().is_err() {
            // Main processors area already borrowed! That's possible in some rare cases.
            // In any case, we need to skip processing. Otherwise, mutable borrow panics would occur!
            //
            // Example:
            // - Preferences => Audio => Recording: Set "Prompt to save/delete/rename new files" to "on stop"
            // - Add mapping that maps key "S" to transport stop action
            // - Start playback
            // - User presses key "S"
            // - RealearnAccelerator borrows the main processors mutably to process the "S" key press to
            // - Mapping triggers transport stop
            // - REAPER shows the modal dialog (prompt)
            // - The main loop continues running on top of the function stack, continuously invoking the run() function
            // - BOOM
            tracing::warn!("Main processors borrowed already. Skipping processing.");
            return;
        }
        // Run
        measure_time("helgobox.control_surface.run", || {
            self.run_internal();
        });
    }

    fn handle_event(&self, event: ControlSurfaceEvent) -> bool {
        // TODO-high-playtime-refactoring We should do this in reaper-medium (in a more generic way) as soon as it turns
        //  out to work nicely. Related to this: https://github.com/helgoboss/reaper-rs/issues/54
        match self.change_event_queue.try_borrow_mut() {
            Ok(mut queue) => self.handle_event_internal(&event, &mut queue),
            Err(_) => {
                // When we can't borrow the control surface event queue because of reentrancy,
                // we need to defer its processing.
                self.control_surface_event_sender
                    .send_complaining(event.clone().into_owned());
                false
            }
        }
    }

    fn get_touch_state(&self, args: GetTouchStateArgs) -> bool {
        if let Ok(domain_type) = TouchedTrackParameterType::try_from_reaper(args.parameter_type) {
            Backbone::target_state()
                .borrow()
                .automation_parameter_is_touched(args.track, domain_type)
        } else {
            false
        }
    }

    fn ext_supports_extended_touch(&self, _: ExtSupportsExtendedTouchArgs) -> i32 {
        1
    }
}

fn process_incoming_osc_packet_for_learning(
    dev_id: OscDeviceId,
    sender: &OscCaptureSender,
    packet: OscPacket,
) {
    match packet {
        OscPacket::Message(msg) => process_incoming_osc_message_for_learning(dev_id, sender, msg),
        OscPacket::Bundle(bundle) => {
            for p in bundle.content.into_iter() {
                process_incoming_osc_packet_for_learning(dev_id, sender, p);
            }
        }
    }
}

fn process_incoming_osc_message_for_learning(
    dev_id: OscDeviceId,
    sender: &OscCaptureSender,
    message: OscMessage,
) {
    let scan_result = OscScanResult {
        message,
        dev_id: Some(dev_id),
    };
    let _ = sender.try_send(scan_result);
}

fn reset_midi_devices(
    in_devs: impl Iterator<Item = MidiInputDeviceId>,
    out_devs: impl Iterator<Item = MidiOutputDeviceId>,
) {
    let reaper_low = Reaper::get().medium_reaper().low();
    if reaper_low.pointers().midi_init.is_none() {
        // REAPER version < 6.47
        return;
    }
    for res in in_devs.zip_longest(out_devs) {
        let (input_arg, output_arg) = match res {
            EitherOrBoth::Both(i, o) => (i.get() as i32, o.get() as i32),
            EitherOrBoth::Left(i) => (i.get() as i32, -1),
            EitherOrBoth::Right(o) => (-1, o.get() as i32),
        };
        reaper_low.midi_init(input_arg, output_arg);
    }
}

fn process_touched_target(
    target: ReaperTarget,
    caused_by_realearn: bool,
    target_capture_senders: &NonCryptoHashMap<Option<UnitId>, TargetCaptureSender>,
) {
    let touch_event = TargetTouchEvent {
        target,
        caused_by_realearn,
    };
    for sender in target_capture_senders.values() {
        let _ = sender.try_send(touch_event.clone());
    }
    Backbone::get().notify_target_touched(touch_event);
}
