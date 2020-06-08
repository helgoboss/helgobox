use c_str_macro::c_str;
use vst::editor::Editor;
use vst::plugin;
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin, PluginParameters};

use super::RealearnEditor;
use crate::domain::{MainProcessorTask, SharedSession};
use crate::domain::{RealTimeProcessor, RealTimeProcessorTask, Session, SessionContext};
use crate::infrastructure::plugin::realearn_plugin_parameters::RealearnPluginParameters;
use crate::infrastructure::ui::MainPanel;
use helgoboss_midi::{RawShortMessage, ShortMessageFactory, U7};
use lazycell::LazyCell;
use reaper_high::{Fx, Project, Reaper, ReaperGuard, Take, Track};
use reaper_low::{reaper_vst_plugin, PluginContext, Swell};
use reaper_medium::{Hz, MidiFrameOffset, TypeSpecificPluginContext};
use rxrust::prelude::*;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;
use std::rc::Rc;
use std::str::Utf8Error;
use std::sync::Arc;
use std::time::Duration;
use swell_ui::SharedView;
use vst::api::{Events, Supported};
use vst::buffer::AudioBuffer;
use vst::event::{Event, MidiEvent};

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
    main_processor_channel: (
        crossbeam_channel::Sender<MainProcessorTask>,
        crossbeam_channel::Receiver<MainProcessorTask>,
    ),
    // Will be cloned to session as soon as it gets created.
    real_time_processor_sender: crossbeam_channel::Sender<RealTimeProcessorTask>,
    // Called in real-time audio thread only.
    // We keep it in this struct in order to be able to inform it about incoming FX MIDI messages
    // without detour.
    real_time_processor: RealTimeProcessor,
}

impl Default for RealearnPlugin {
    fn default() -> Self {
        let (real_time_processor_sender, real_time_processor_receiver) =
            crossbeam_channel::unbounded();
        let (main_processor_sender, main_processor_receiver) = crossbeam_channel::unbounded();
        Self {
            host: Default::default(),
            session: Rc::new(LazyCell::new()),
            main_panel: Default::default(),
            reaper_guard: None,
            plugin_parameters: Default::default(),
            real_time_processor_sender,
            main_processor_channel: (main_processor_sender.clone(), main_processor_receiver),
            real_time_processor: RealTimeProcessor::new(
                real_time_processor_receiver,
                main_processor_sender,
                HostCallback::default(),
            ),
        }
    }
}

impl Plugin for RealearnPlugin {
    fn new(host: HostCallback) -> Self {
        let default = RealearnPlugin::default();
        Self {
            host,
            real_time_processor: RealTimeProcessor {
                host,
                ..default.real_time_processor
            },
            ..default
        }
    }

    fn get_info(&self) -> Info {
        Info {
            name: "realearn-rs".to_string(),
            unique_id: 2964,
            preset_chunks: true,
            category: Category::Synth,
            ..Default::default()
        }
    }

    fn init(&mut self) {
        firewall(|| {
            self.reaper_guard = Some(self.ensure_reaper_setup());
            self.schedule_session_creation();
        });
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        Some(Box::new(RealearnEditor::new(self.main_panel.clone())))
    }

    fn can_do(&self, can_do: CanDo) -> Supported {
        use CanDo::*;
        use Supported::*;
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
    }

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        self.plugin_parameters.clone()
    }

    fn vendor_specific(&mut self, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
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
    }

    fn process_events(&mut self, events: &Events) {
        for e in events.events() {
            match e {
                Event::Midi(me) => {
                    let msg = RawShortMessage::from_bytes((
                        me.data[0],
                        U7::new(me.data[1]),
                        U7::new(me.data[2]),
                    ))
                    .expect("received invalid MIDI message");
                    // This is called in real-time audio thread, so we can just call the real-time
                    // processor.
                    let offset = MidiFrameOffset::new(
                        u32::try_from(me.delta_frames).expect("negative MIDI frame offset"),
                    );
                    self.real_time_processor
                        .process_incoming_midi_from_fx_input(offset, msg);
                }
                _ => (),
            }
        }
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        // This is called in real-time audio thread, so we can just call the real-time processor.
        self.real_time_processor.idle(buffer.samples());
    }

    fn set_sample_rate(&mut self, rate: f32) {
        // This is called in main thread, so we need to send it to the real-time processor via
        // channel. Real-time processor needs sample rate to do some MIDI clock calculations.
        self.real_time_processor_sender
            .send(RealTimeProcessorTask::UpdateSampleRate(Hz::new(rate as _)));
    }

    fn resume(&mut self) {
        // REAPER usually suspends and resumes whenever starting to play.
        self.main_processor_channel
            .0
            .send(MainProcessorTask::FeedbackAll);
    }
}

impl RealearnPlugin {
    fn ensure_reaper_setup(&mut self) -> Arc<ReaperGuard> {
        Reaper::guarded(|| {
            // Done once for all ReaLearn instances
            let context =
                PluginContext::from_vst_plugin(&self.host, reaper_vst_plugin::static_context())
                    .unwrap();
            Swell::make_available_globally(Swell::load(context));
            Reaper::setup_with_defaults(context, "info@helgoboss.org");
            let reaper = Reaper::get();
            reaper.activate();
        })
    }

    /// At this point, REAPER cannot reliably give use yet the containing FX. As a
    /// consequence we also don't have a session yet, because creating an incomplete session
    /// pushes the problem of not knowing the containing FX into the application logic, which
    /// we for sure don't want. In the next main loop cycle, it should be possible to
    /// identify the containing FX.
    fn schedule_session_creation(&self) {
        let main_panel = self.main_panel.clone();
        let session_container = self.session.clone();
        let plugin_parameters = self.plugin_parameters.clone();
        let host = self.host;
        let real_time_sender = self.real_time_processor_sender.clone();
        let main_processor_channel = self.main_processor_channel.clone();
        Reaper::get().do_later_in_main_thread_asap(move || {
            let session_context = SessionContext::from_host(&host);
            let session = Session::new(session_context, real_time_sender, main_processor_channel);
            let shared_session = Rc::new(RefCell::new(session));
            Session::activate(shared_session.clone());
            main_panel.notify_session_is_available(shared_session.clone());
            plugin_parameters.notify_session_is_available(shared_session.clone());
            session_container.fill(shared_session);
        });
    }

    fn get_named_config_param(&self, param_name: &str, buffer: &mut [c_char]) -> bool {
        if buffer.len() < 1 {
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

fn firewall<F: FnOnce() -> R, R>(f: F) -> Option<R> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}
