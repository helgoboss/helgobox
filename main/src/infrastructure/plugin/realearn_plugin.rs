use vst::editor::Editor;
use vst::plugin;
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin, PluginParameters};

use super::RealearnEditor;
use crate::domain::{
    ControlMainTask, FeedbackRealTimeTask, NormalMainTask, ProcessorContext, PLUGIN_PARAMETER_COUNT,
};
use crate::domain::{NormalRealTimeTask, RealTimeProcessor};
use crate::infrastructure::plugin::debug_util;
use crate::infrastructure::plugin::realearn_plugin_parameters::RealearnPluginParameters;
use crate::infrastructure::ui::MainPanel;
use helgoboss_midi::{RawShortMessage, ShortMessageFactory, U7};
use lazycell::LazyCell;
use reaper_high::{Reaper, ReaperGuard};
use reaper_low::{reaper_vst_plugin, static_vst_plugin_context, PluginContext, Swell};
use reaper_medium::{Hz, MessageBoxType, MidiFrameOffset};

use slog::debug;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use std::rc::Rc;

use std::sync::Arc;

use crate::application::{session_manager, Session, SharedSession};
use crate::infrastructure::plugin::app::App;
use swell_ui::SharedView;
use vst::api::{Events, Supported};
use vst::buffer::AudioBuffer;
use vst::event::Event;

reaper_vst_plugin!();

pub struct RealearnPlugin {
    // This will be filled right at construction time. It won't have a session yet though.
    main_panel: SharedView<MainPanel>,
    // This will be set on `new()`.
    host: HostCallback,
    // This will be set as soon as the containing FX is known (one main loop cycle after `init()`).
    session: Rc<LazyCell<SharedSession>>,
    // We need to keep that here in order to notify it as soon as the session becomes available.
    plugin_parameters: Arc<RealearnPluginParameters>,
    // This will be set on `init()`.
    reaper_guard: Option<Arc<ReaperGuard>>,
    // Will be cloned to session as soon as it gets created.
    normal_main_task_channel: (
        crossbeam_channel::Sender<NormalMainTask>,
        crossbeam_channel::Receiver<NormalMainTask>,
    ),
    // Will be cloned to session as soon as it gets created.
    control_main_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    // Will be cloned to session as soon as it gets created.
    normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
    // Will be cloned to session as soon as it gets created.
    feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
    // Called in real-time audio thread only.
    // We keep it in this struct in order to be able to inform it about incoming FX MIDI messages
    // without detour.
    real_time_processor: RealTimeProcessor,
}

impl Default for RealearnPlugin {
    fn default() -> Self {
        RealearnPlugin::new(Default::default())
    }
}

impl Plugin for RealearnPlugin {
    fn new(host: HostCallback) -> Self {
        firewall(|| {
            // TODO-low Unbounded? Brave.
            let (normal_real_time_task_sender, normal_real_time_task_receiver) =
                crossbeam_channel::unbounded();
            let (feedback_real_time_task_sender, feedback_real_time_task_receiver) =
                crossbeam_channel::unbounded();
            let (normal_main_task_sender, normal_main_task_receiver) =
                crossbeam_channel::unbounded();
            let (control_main_task_sender, control_main_task_receiver) =
                crossbeam_channel::unbounded();
            Self {
                host,
                session: Rc::new(LazyCell::new()),
                main_panel: Default::default(),
                reaper_guard: None,
                plugin_parameters: Default::default(),
                normal_real_time_task_sender,
                feedback_real_time_task_sender,
                normal_main_task_channel: (
                    normal_main_task_sender.clone(),
                    normal_main_task_receiver,
                ),
                real_time_processor: RealTimeProcessor::new(
                    normal_real_time_task_receiver,
                    feedback_real_time_task_receiver,
                    normal_main_task_sender,
                    control_main_task_sender,
                    host,
                ),
                control_main_task_receiver,
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
                ..Default::default()
            }
        })
        .unwrap_or_default()
    }

    fn init(&mut self) {
        firewall(|| {
            self.reaper_guard = Some(self.ensure_reaper_setup());
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
                SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent => Supported::Yes,
                // If we don't do this, REAPER for Linux won't give us a SWELL plug-in window, which
                // leads to a horrible crash when doing CreateDialogParam. In our UI we use SWELL
                // to put controls into the plug-in window. SWELL assumes that the parent window for
                // controls is also a SWELL window.
                Other(s) => match s.as_str() {
                    "hasCockosViewAsConfig" => Custom(0xbeef_0000),
                    "hasCockosExtensions" => Custom(0xbeef_0000),
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
            let opcode = plugin::OpCode::from(index);
            use plugin::OpCode::*;
            match opcode {
                // Cockos named_parameter_name (http://reaper.fm/sdk/vst/vst_ext.php)
                GetData if value != 0 => {
                    let param_name = unsafe { CStr::from_ptr(value as *const c_char) };
                    let param_name = match param_name.to_str() {
                        Ok(n) => n,
                        Err(_) => return 0,
                    };
                    let buffer =
                        unsafe { std::slice::from_raw_parts_mut(ptr as *mut c_char, opt as _) };
                    let supported = self.get_named_config_param(param_name, buffer);
                    if supported { 0xf00d } else { 0 }
                }
                _ => 0,
            }
        })
        .unwrap_or(0)
    }

    fn process_events(&mut self, events: &Events) {
        firewall(|| {
            for e in events.events() {
                if let Event::Midi(me) = e {
                    let msg = RawShortMessage::from_bytes((
                        me.data[0],
                        U7::new(me.data[1]),
                        U7::new(me.data[2]),
                    ))
                    .expect("received invalid MIDI message");
                    // This is called in real-time audio thread, so we can just call the
                    // real-time processor.
                    let offset = MidiFrameOffset::new(
                        u32::try_from(me.delta_frames).expect("negative MIDI frame offset"),
                    );
                    self.real_time_processor
                        .process_incoming_midi_from_fx_input(offset, msg);
                }
            }
        });
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        firewall(|| {
            // This is called in real-time audio thread, so we can just call the real-time
            // processor.
            self.real_time_processor.idle(buffer.samples());
        });
    }

    fn set_sample_rate(&mut self, rate: f32) {
        firewall(|| {
            // This is called in main thread, so we need to send it to the real-time processor via
            // channel. Real-time processor needs sample rate to do some MIDI clock calculations.
            // If task queue is full, don't spam user with error messages.
            let _ = self
                .normal_real_time_task_sender
                .send(NormalRealTimeTask::UpdateSampleRate(Hz::new(rate as _)));
        });
    }

    fn resume(&mut self) {
        firewall(|| {
            // REAPER usually suspends and resumes whenever starting to play.
            // If task queue is full, don't spam user with error messages.
            let _ = self
                .normal_main_task_channel
                .0
                .send(NormalMainTask::FeedbackAll);
        });
    }
}

impl RealearnPlugin {
    fn ensure_reaper_setup(&mut self) -> Arc<ReaperGuard> {
        Reaper::guarded(|| {
            let context =
                PluginContext::from_vst_plugin(&self.host, static_vst_plugin_context()).unwrap();
            Swell::make_available_globally(Swell::load(context));
            Reaper::setup_with_defaults(context, "info@helgoboss.org");
            session_manager::register_global_learn_action();
            debug_util::register_resolve_symbols_action();
        })
    }

    /// At this point, REAPER cannot reliably give us yet the containing FX. As a
    /// consequence we also don't have a session yet, because creating an incomplete session
    /// pushes the problem of not knowing the containing FX into the application logic, which
    /// we for sure don't want. In the next main loop cycle, it should be possible to
    /// identify the containing FX.
    fn schedule_session_creation(&self) {
        let main_panel = self.main_panel.clone();
        let session_container = self.session.clone();
        let plugin_parameters = self.plugin_parameters.clone();
        let host = self.host;
        let normal_real_time_task_sender = self.normal_real_time_task_sender.clone();
        let feedback_real_time_task_sender = self.feedback_real_time_task_sender.clone();
        let normal_main_task_channel = self.normal_main_task_channel.clone();
        let control_main_task_receiver = self.control_main_task_receiver.clone();
        Reaper::get()
            .do_later_in_main_thread_asap(move || {
                let processor_context = match ProcessorContext::from_host(&host) {
                    Ok(c) => c,
                    Err(msg) => {
                        Reaper::get().medium_reaper().show_message_box(
                            msg,
                            "ReaLearn",
                            MessageBoxType::Okay,
                        );
                        return;
                    }
                };
                let session = Session::new(
                    processor_context,
                    normal_real_time_task_sender,
                    feedback_real_time_task_sender,
                    normal_main_task_channel,
                    control_main_task_receiver,
                    // It's important that we use a weak pointer here. Otherwise the session keeps
                    // a strong reference to the UI and the UI keeps strong
                    // references to the session. This results in UI stuff not
                    // being dropped when the plug-in is removed. It
                    // doesn't result in a crash, but there's no cleanup.
                    Rc::downgrade(&main_panel),
                    App::get().controller_manager(),
                );
                let shared_session = Rc::new(RefCell::new(session));
                let weak_session = Rc::downgrade(&shared_session);
                session_manager::register_session(weak_session.clone());
                shared_session.borrow_mut().activate(weak_session.clone());
                main_panel.notify_session_is_available(weak_session.clone());
                plugin_parameters.notify_session_is_available(weak_session);
                // RealearnPlugin is the main owner of the session. Everywhere else the session is
                // just temporarily upgraded, never stored as Rc, only as Weak.
                session_container.fill(shared_session).unwrap();
            })
            .unwrap();
    }

    fn get_named_config_param(&self, param_name: &str, buffer: &mut [c_char]) -> bool {
        if buffer.is_empty() {
            return false;
        }
        match param_name {
            crate::domain::WAITING_FOR_SESSION_PARAM_NAME => {
                buffer[0] = if self.session.filled() { 0 } else { 1 };
                true
            }
            _ => false,
        }
    }
}

impl Drop for RealearnPlugin {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping plug-in...");
        if let Some(session) = self.session.borrow() {
            session_manager::unregister_session(session.as_ptr());
            debug!(
                Reaper::get().logger(),
                "{} pointers are still referring to this session",
                Rc::strong_count(session)
            );
        }
    }
}

fn firewall<F: FnOnce() -> R, R>(f: F) -> Option<R> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}
