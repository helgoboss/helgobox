use vst::editor::Editor;
use vst::plugin;
use vst::plugin::{
    CanDo, Category, HostCallback, Info, Plugin, PluginParameterCharacter, PluginParameterInfo,
    PluginParameters,
};

use super::RealearnEditor;
use crate::domain::{
    AudioBlockProps, BackboneState, ControlEvent, ControlEventTimestamp, ControlMainTask,
    FeedbackRealTimeTask, InstanceId, MainProcessor, MidiEvent, NormalMainTask,
    NormalRealTimeToMainThreadTask, ParameterMainTask, PluginParamIndex, ProcessorContext,
    RealTimeProcessorLocker, SharedRealTimeProcessor, PLUGIN_PARAMETER_COUNT,
};
use crate::domain::{NormalRealTimeTask, RealTimeProcessor};
use crate::infrastructure::plugin::realearn_plugin_parameters::RealearnPluginParameters;
use crate::infrastructure::plugin::SET_STATE_PARAM_NAME;
use crate::infrastructure::ui::MainPanel;
use assert_no_alloc::*;
use base::{
    tracing_debug, Global, NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread,
};
use lazycell::LazyCell;
use reaper_high::{Reaper, ReaperGuard};
use reaper_low::{reaper_vst_plugin, static_vst_plugin_context, PluginContext};
use reaper_medium::{Hz, ReaperStr};

use slog::{debug, o};
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use std::rc::Rc;

use std::sync::{Arc, Mutex};

use crate::application::{Session, SharedSession};
use crate::infrastructure::plugin::app::App;

use crate::base::notification;
use crate::infrastructure::server::http::keep_informing_clients_about_session_events;
use helgoboss_learn::AbstractTimestamp;
use std::convert::TryInto;
use std::slice;
use swell_ui::SharedView;
use vst::api::{Events, Supported};
use vst::buffer::AudioBuffer;
use vst::host::Host;

const NORMAL_REAL_TIME_TASK_QUEUE_SIZE: usize = 1000;
const FEEDBACK_REAL_TIME_TASK_QUEUE_SIZE: usize = 2000;
const NORMAL_REAL_TIME_TO_MAIN_TASK_QUEUE_SIZE: usize = 10_000;
const CONTROL_MAIN_TASK_QUEUE_SIZE: usize = 5000;
const PARAMETER_MAIN_TASK_QUEUE_SIZE: usize = 5000;

reaper_vst_plugin!();

pub struct RealearnPlugin {
    /// An ID which is randomly generated on each start and is most relevant for log correlation.
    /// It's also used in other ReaLearn singletons.
    /// It also serves as initial value for the (persistent) session ID. Should be unique.
    instance_id: InstanceId,
    logger: slog::Logger,
    // This will be filled right at construction time. It won't have a session yet though.
    main_panel: SharedView<MainPanel>,
    // This will be set on `new()`.
    host: HostCallback,
    // This will be set as soon as the containing FX is known (one main loop cycle after `init()`).
    session: Rc<LazyCell<SharedSession>>,
    // We need to keep that here in order to notify it as soon as the session becomes available.
    plugin_parameters: Arc<RealearnPluginParameters>,
    // This will be set on `init()`.
    _reaper_guard: Option<Arc<ReaperGuard>>,
    // Will be cloned to session as soon as it gets created.
    normal_main_task_channel: (
        SenderToNormalThread<NormalMainTask>,
        crossbeam_channel::Receiver<NormalMainTask>,
    ),
    // Will be cloned to session as soon as it gets created.
    control_main_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    // Will be cloned to session as soon as it gets created.
    normal_rt_to_main_task_receiver: crossbeam_channel::Receiver<NormalRealTimeToMainThreadTask>,
    // Will be cloned to session as soon as it gets created.
    parameter_main_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
    // Will be cloned to session as soon as it gets created.
    normal_real_time_task_sender: SenderToRealTimeThread<NormalRealTimeTask>,
    // Will be cloned to session as soon as it gets created.
    feedback_real_time_task_sender: SenderToRealTimeThread<FeedbackRealTimeTask>,
    // Called in real-time audio thread only.
    // We keep it in this struct in order to be able to inform it about incoming FX MIDI messages
    // and drive its processing without detour. Well, almost. We share it with the global ReaLearn
    // audio hook that also drives processing (because in some cases the VST processing is
    // stopped). That's why we need an Rc/RefCell.
    real_time_processor: SharedRealTimeProcessor,
    // For detecting play state changes
    was_playing_in_last_cycle: bool,
    sample_rate: Hz,
}

impl Default for RealearnPlugin {
    fn default() -> Self {
        RealearnPlugin::new(Default::default())
    }
}

unsafe impl Send for RealearnPlugin {}

impl Plugin for RealearnPlugin {
    fn new(host: HostCallback) -> Self {
        firewall(|| {
            let (normal_real_time_task_sender, normal_real_time_task_receiver) =
                SenderToRealTimeThread::new_channel(
                    "normal real-time tasks",
                    NORMAL_REAL_TIME_TASK_QUEUE_SIZE,
                );
            let (feedback_real_time_task_sender, feedback_real_time_task_receiver) =
                SenderToRealTimeThread::new_channel(
                    "feedback real-time tasks",
                    FEEDBACK_REAL_TIME_TASK_QUEUE_SIZE,
                );
            let (normal_main_task_sender, normal_main_task_receiver) =
                SenderToNormalThread::new_unbounded_channel("normal main tasks");
            let (normal_rt_to_main_task_sender, normal_rt_to_main_task_receiver) =
                SenderToNormalThread::new_bounded_channel(
                    "normal real-time to main tasks",
                    NORMAL_REAL_TIME_TO_MAIN_TASK_QUEUE_SIZE,
                );
            let (control_main_task_sender, control_main_task_receiver) =
                SenderToNormalThread::new_bounded_channel(
                    "control main tasks",
                    CONTROL_MAIN_TASK_QUEUE_SIZE,
                );
            let (parameter_main_task_sender, parameter_main_task_receiver) =
                SenderToNormalThread::new_bounded_channel(
                    "parameter main tasks",
                    PARAMETER_MAIN_TASK_QUEUE_SIZE,
                );
            let instance_id = InstanceId::random();
            let logger = App::logger().new(o!("instance" => instance_id.to_string()));
            let plugin_parameters =
                Arc::new(RealearnPluginParameters::new(parameter_main_task_sender));
            let real_time_processor = RealTimeProcessor::new(
                instance_id,
                &logger,
                normal_real_time_task_receiver,
                feedback_real_time_task_receiver,
                feedback_real_time_task_sender.clone(),
                normal_rt_to_main_task_sender,
                control_main_task_sender,
                App::garbage_bin().clone(),
            );
            let real_time_processor = Arc::new(Mutex::new(real_time_processor));
            // This is necessary since Rust 1.62.0 (or 1.63.0, not sure). Since those versions,
            // locking a mutex the first time apparently allocates. If we don't lock the
            // mutex now for the first time but do it in the real-time thread, assert_no_alloc will
            // complain in debug builds.
            drop(real_time_processor.lock_recover());
            Self {
                instance_id,
                logger: logger.clone(),
                host,
                session: Rc::new(LazyCell::new()),
                main_panel: SharedView::new(MainPanel::new(Arc::downgrade(&plugin_parameters))),
                _reaper_guard: None,
                plugin_parameters,
                normal_real_time_task_sender,
                feedback_real_time_task_sender,
                normal_main_task_channel: (normal_main_task_sender, normal_main_task_receiver),
                real_time_processor,
                parameter_main_task_receiver,
                control_main_task_receiver,
                normal_rt_to_main_task_receiver,
                was_playing_in_last_cycle: false,
                sample_rate: Default::default(),
            }
        })
        .unwrap_or_default()
    }

    fn get_info(&self) -> Info {
        firewall(|| {
            Info {
                name: "ReaLearn".to_string(),
                vendor: "Helgoboss".to_string(),
                // In C++ this is the same like 'hbrl'
                unique_id: 1751282284,
                preset_chunks: true,
                category: Category::Synth,
                parameters: PLUGIN_PARAMETER_COUNT as i32,
                f64_precision: true,
                inputs: 2,
                outputs: 0,
                ..Default::default()
            }
        })
        .unwrap_or_default()
    }

    fn get_parameter_info(&self, index: i32) -> Option<PluginParameterInfo> {
        let i = match PluginParamIndex::try_from(index as u32) {
            Ok(i) => i,
            Err(_) => return None,
        };
        let params = self.plugin_parameters.params();
        let param = params.at(i);
        if let Some(value_count) = param.setting().value_count {
            let mut info = PluginParameterInfo::default();
            info.character = PluginParameterCharacter::Discrete {
                min: 0,
                max: (value_count.get() - 1) as i32,
                steps: None,
            };
            Some(info)
        } else {
            None
        }
    }

    fn init(&mut self) {
        firewall(|| {
            self._reaper_guard = Some(self.ensure_reaper_setup());
            self.schedule_session_creation();
        });
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        firewall(|| {
            let boxed: Box<dyn Editor> = Box::new(RealearnEditor::new(self.main_panel.clone()));
            Some(boxed)
        })
        .unwrap_or(None)
    }

    fn can_do(&self, can_do: CanDo) -> Supported {
        firewall(|| {
            use CanDo::*;
            use Supported::*;
            #[allow(overflowing_literals)]
            match can_do {
                SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent
                | ReceiveSysExEvent => Supported::Yes,
                // If we don't do this, REAPER for Linux won't give us a SWELL plug-in window, which
                // leads to a horrible crash when doing CreateDialogParam. In our UI we use SWELL
                // to put controls into the plug-in window. SWELL assumes that the parent window for
                // controls is also a SWELL window.
                Other(s) => match s.as_str() {
                    "hasCockosViewAsConfig" => Custom(0xbeef_0000),
                    "hasCockosExtensions" => Custom(0xbeef_0000),
                    // This is necessary for REAPER 6.48 - 6.51 on macOS to not let the background
                    // turn black. These REAPER versions introduced a change putting third-party
                    // VSTs into a container window. The following line prevents that. For
                    // REAPER v6.52+ it's not necessary anymore because it also reacts to
                    // "hasCockosViewAsConfig".
                    "hasCockosNoScrollUI" => Custom(0xbeef_0000),
                    _ => Maybe,
                },
                _ => Maybe,
            }
        })
        .unwrap_or(Supported::No)
    }

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        self.plugin_parameters.clone()
    }

    fn vendor_specific(&mut self, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
        firewall(|| {
            // tracing_debug!("VST vendor specific (index = {})", index);
            let opcode: plugin::OpCode = match index.try_into() {
                Ok(c) => c,
                Err(_) => return 0,
            };
            self.handle_vendor_specific(opcode, value, ptr, opt)
        })
        .unwrap_or(0)
    }

    fn process_events(&mut self, events: &Events) {
        firewall(|| {
            assert_no_alloc(|| {
                let timestamp = ControlEventTimestamp::now();
                let is_transport_start = !self.was_playing_in_last_cycle && self.is_now_playing();
                for e in events.events() {
                    let our_event = match MidiEvent::from_vst(e) {
                        Err(_) => {
                            // Just ignore if not a valid MIDI message. Invalid MIDI message was
                            // observed in the wild: https://github.com/helgoboss/realearn/issues/82.
                            continue;
                        }
                        Ok(e) => e,
                    };
                    let our_event = ControlEvent::new(our_event, timestamp);
                    // This is called in real-time audio thread, so we can just call the
                    // real-time processor.
                    self.real_time_processor
                        .lock_recover()
                        .process_incoming_midi_from_vst(our_event, is_transport_start, &self.host);
                }
            });
        });
    }

    fn process_f64(&mut self, buffer: &mut AudioBuffer<f64>) {
        assert_no_alloc(|| {
            // Get current time information so we can detect changes in play state reliably
            // (TimeInfoFlags::TRANSPORT_CHANGED doesn't work the way we want it).
            self.was_playing_in_last_cycle = self.is_now_playing();
            let block_props = AudioBlockProps::from_vst(buffer, self.sample_rate);
            self.real_time_processor
                .lock_recover()
                .run_from_vst(buffer, block_props, &self.host);
        });
    }

    fn set_sample_rate(&mut self, rate: f32) {
        firewall(|| {
            tracing_debug!("VST set sample rate");
            self.sample_rate = Hz::new(rate as _);
            // This is called in main thread, so we need to send it to the real-time processor via
            // channel. Real-time processor needs sample rate to do some MIDI clock calculations.
            // If task queue is full or audio not running, so what. Don't spam the user with error
            // messages.
            self.normal_real_time_task_sender
                .send_if_space(NormalRealTimeTask::UpdateSampleRate(Hz::new(rate as _)));
        });
    }

    fn suspend(&mut self) {
        tracing_debug!("VST suspend");
    }

    fn resume(&mut self) {
        tracing_debug!("VST resume");
    }

    fn set_block_size(&mut self, _size: i64) {
        tracing_debug!("VST set block size");
    }

    fn start_process(&mut self) {
        tracing_debug!("VST start process");
    }

    fn stop_process(&mut self) {
        tracing_debug!("VST stop process");
    }
}

impl RealearnPlugin {
    /// Should be called in real-time thread only.
    fn is_now_playing(&self) -> bool {
        use vst::api::TimeInfoFlags;
        let time_info = self
            .host
            .get_time_info(TimeInfoFlags::TRANSPORT_PLAYING.bits());
        match time_info {
            None => false,
            Some(ti) => {
                let flags = TimeInfoFlags::from_bits_truncate(ti.flags);
                flags.intersects(TimeInfoFlags::TRANSPORT_PLAYING)
            }
        }
    }

    fn ensure_reaper_setup(&mut self) -> Arc<ReaperGuard> {
        Reaper::guarded(
            || {
                let context =
                    PluginContext::from_vst_plugin(&self.host, static_vst_plugin_context())
                        .unwrap();
                App::init_static(self.logger.clone(), context);
            },
            || {
                App::get().wake_up();
                || {
                    App::get().go_to_sleep();
                }
            },
        )
    }

    /// At this point, REAPER cannot reliably give us yet the containing FX. As a
    /// consequence we also don't have a session yet, because creating an incomplete session
    /// pushes the problem of not knowing the containing FX into the application logic, which
    /// we for sure don't want. In the next main loop cycle, it should be possible to
    /// identify the containing FX.
    // TODO-low An alternative for cloning all this stuff would be to introduce a state machine
    //  just like in infrastructure::App.
    fn schedule_session_creation(&self) {
        let main_panel = self.main_panel.clone();
        let session_container = self.session.clone();
        let plugin_parameters = self.plugin_parameters.clone();
        let host = self.host;
        let normal_real_time_task_sender = self.normal_real_time_task_sender.clone();
        let shared_real_time_processor = self.real_time_processor.clone();
        let feedback_real_time_task_sender = self.feedback_real_time_task_sender.clone();
        let normal_main_task_channel = self.normal_main_task_channel.clone();
        let control_main_task_receiver = self.control_main_task_receiver.clone();
        let parameter_main_task_receiver = self.parameter_main_task_receiver.clone();
        let normal_rt_to_main_task_receiver = self.normal_rt_to_main_task_receiver.clone();
        let logger = self.logger.clone();
        let instance_id = self.instance_id;
        Global::task_support()
            .do_later_in_main_thread_from_main_thread_asap(move || {
                let processor_context = match ProcessorContext::from_host(host) {
                    Ok(c) => c,
                    Err(msg) => {
                        notification::alert(msg);
                        return;
                    }
                };
                // Instance state (domain - shared)
                let (instance_feedback_event_sender, instance_feedback_event_receiver) =
                    SenderToNormalThread::new_unbounded_channel("instance state change events");
                let instance_state = BackboneState::get().create_instance(
                    instance_id,
                    processor_context.clone(),
                    instance_feedback_event_sender,
                    App::get().clip_matrix_event_sender().clone(),
                    App::get().normal_audio_hook_task_sender().clone(),
                    normal_real_time_task_sender.clone(),
                );
                // Session (application - shared)
                let session = Session::new(
                    instance_id,
                    &logger,
                    processor_context.clone(),
                    normal_real_time_task_sender.clone(),
                    normal_main_task_channel.0.clone(),
                    // It's important that we use a weak pointer here. Otherwise the session keeps
                    // a strong reference to the UI and the UI keeps strong
                    // references to the session. This results in UI stuff not
                    // being dropped when the plug-in is removed. It
                    // doesn't result in a crash, but there's no cleanup.
                    Rc::downgrade(&main_panel),
                    plugin_parameters.clone(),
                    App::get(),
                    App::get().controller_preset_manager(),
                    App::get().main_preset_manager(),
                    App::get().preset_link_manager(),
                    instance_state.clone(),
                    App::get().feedback_audio_hook_task_sender(),
                    feedback_real_time_task_sender.clone(),
                    App::get().osc_feedback_task_sender(),
                    App::get().control_surface_main_task_sender(),
                );
                let shared_session = Rc::new(RefCell::new(session));
                let weak_session = Rc::downgrade(&shared_session);
                keep_informing_clients_about_session_events(&shared_session);
                App::get().register_session(weak_session.clone());
                // Main processor - (domain, owned by REAPER control surface)
                // Register the main processor with the global ReaLearn control surface. We let it
                // call by the control surface because it must be called regularly,
                // even when the ReaLearn UI is closed. That means, the VST GUI idle
                // callback is not suited.
                let main_processor = MainProcessor::new(
                    instance_id,
                    &logger,
                    normal_main_task_channel.0.clone(),
                    normal_main_task_channel.1,
                    normal_rt_to_main_task_receiver,
                    parameter_main_task_receiver,
                    control_main_task_receiver,
                    instance_feedback_event_receiver,
                    normal_real_time_task_sender,
                    feedback_real_time_task_sender,
                    App::get().feedback_audio_hook_task_sender().clone(),
                    App::get().additional_feedback_event_sender(),
                    App::get().instance_orchestration_event_sender(),
                    App::get().osc_feedback_task_sender().clone(),
                    weak_session.clone(),
                    processor_context,
                    instance_state,
                    App::get(),
                );
                App::get().register_processor_couple(
                    instance_id,
                    shared_real_time_processor,
                    main_processor,
                );
                shared_session.borrow_mut().activate(weak_session.clone());
                main_panel.notify_session_is_available(weak_session.clone());
                plugin_parameters.notify_session_is_available(weak_session);
                shared_session.borrow().notify_realearn_instance_started();
                // RealearnPlugin is the main owner of the session. Everywhere else the session is
                // just temporarily upgraded, never stored as Rc, only as Weak.
                session_container.fill(shared_session).unwrap();
            })
            .unwrap();
    }

    fn get_named_config_param(
        &self,
        param_name: &str,
        buffer: &mut [c_char],
    ) -> Result<(), &'static str> {
        if buffer.is_empty() {
            return Err("empty buffer");
        }
        match param_name {
            crate::domain::WAITING_FOR_SESSION_PARAM_NAME => {
                buffer[0] = if self.session.filled() { 0 } else { 1 };
                Ok(())
            }
            _ => Err("unhandled config param"),
        }
    }

    fn set_named_config_param(
        &self,
        param_name: &str,
        buffer: *const c_char,
    ) -> Result<(), &'static str> {
        match param_name {
            SET_STATE_PARAM_NAME => {
                let c_str = unsafe { CStr::from_ptr(buffer) };
                let rust_str = c_str.to_str().expect("not valid UTF-8");
                self.plugin_parameters.load_state(rust_str);
                Ok(())
            }
            _ => Err("unhandled config param"),
        }
    }

    fn handle_vendor_specific(
        &mut self,
        opcode: plugin::OpCode,
        value: isize,
        ptr: *mut c_void,
        opt: f32,
    ) -> isize {
        use plugin::OpCode::*;
        fn interpret_as_param_name(value: isize) -> Result<&'static str, &'static str> {
            let param_name = unsafe { CStr::from_ptr(value as *const c_char) };
            param_name.to_str().map_err(|_| "invalid parameter name")
        }
        match opcode {
            // Cockos: named_parameter_name (http://reaper.fm/sdk/vst/vst_ext.php)
            GetData if value != 0 => {
                let param_name = match interpret_as_param_name(value) {
                    Ok(n) => n,
                    Err(_) => return 0,
                };
                let buffer =
                    unsafe { std::slice::from_raw_parts_mut(ptr as *mut c_char, opt as _) };
                if self.get_named_config_param(param_name, buffer).is_ok() {
                    0xf00d
                } else {
                    0
                }
            }
            // Cockos: named_parameter_name (http://reaper.fm/sdk/vst/vst_ext.php)
            SetData if value != 0 => {
                let param_name = match interpret_as_param_name(value) {
                    Ok(n) => n,
                    Err(_) => return 0,
                };
                if self
                    .set_named_config_param(param_name, ptr as *const c_char)
                    .is_ok()
                {
                    0xf00d
                } else {
                    0
                }
            }
            // Cockos: Format parameter value without setting it (http://reaper.fm/sdk/vst/vst_ext.php)
            GetParameterDisplay if !ptr.is_null() && value >= 0 => {
                let i = match PluginParamIndex::try_from(value as u32) {
                    Ok(i) => i,
                    Err(_) => return 0,
                };
                let params = self.plugin_parameters.params();
                let string = params.at(i).setting().with_raw_value(opt).to_string();
                if write_to_c_str(ptr, string).is_err() {
                    return 0;
                }
                0xbeef
            }
            // Cockos: Parse parameter value without setting it (http://reaper.fm/sdk/vst/vst_ext.php)
            StringToParameter if !ptr.is_null() && value >= 0 => {
                let i = match PluginParamIndex::try_from(value as u32) {
                    Ok(i) => i,
                    Err(_) => return 0,
                };
                let reaper_str = unsafe { ReaperStr::from_ptr(ptr as *const c_char) };
                let text_input = reaper_str.to_str();
                if text_input.is_empty() {
                    // REAPER checks if we support this.
                    return 0xbeef;
                }
                let params = self.plugin_parameters.params();
                let param = params.at(i);
                let raw_value = match param.setting().parse_to_raw_value(text_input) {
                    Ok(v) => v,
                    Err(_) => return 0,
                };
                if write_to_c_str(ptr, raw_value.to_string()).is_err() {
                    return 0;
                }
                0xbeef
            }
            _ => 0,
        }
    }
}

fn write_to_c_str(dest: *mut c_void, src: String) -> Result<(), &'static str> {
    let c_string = match CString::new(src) {
        Ok(s) => s,
        Err(_) => return Err("Rust string contained nul byte"),
    };
    let bytes = c_string.as_bytes_with_nul();
    let dest_slice = unsafe { slice::from_raw_parts_mut(dest as *mut u8, 256) };
    dest_slice[..bytes.len()].copy_from_slice(bytes);
    Ok(())
}

impl Drop for RealearnPlugin {
    fn drop(&mut self) {
        debug!(self.logger, "Dropping plug-in...");
        if let Some(session) = self.session.borrow() {
            App::get().unregister_processor_couple(self.instance_id);
            App::get().unregister_session(session.as_ptr());
            debug!(
                self.logger,
                "{} pointers are still referring to this session",
                Rc::strong_count(session)
            );
        }
    }
}

fn firewall<F: FnOnce() -> R, R>(f: F) -> Option<R> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}
