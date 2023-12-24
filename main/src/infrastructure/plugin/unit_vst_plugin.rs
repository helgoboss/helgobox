use vst::editor::Editor;
use vst::plugin;
use vst::plugin::{
    CanDo, Category, HostCallback, Info, Plugin, PluginParameterCharacter, PluginParameterInfo,
    PluginParameters,
};

use crate::domain::{
    ControlEvent, ControlEventTimestamp, MidiEvent, PluginParamIndex, ProcessorContext, UnitId,
    PLUGIN_PARAMETER_COUNT,
};
use crate::infrastructure::plugin::unit_parameter_container::UnitParameterContainer;
use crate::infrastructure::plugin::SET_STATE_PARAM_NAME;
use base::{tracing_debug, Global};
use helgoboss_allocator::*;
use reaper_high::{Reaper, ReaperGuard};
use reaper_low::{static_vst_plugin_context, PluginContext};
use reaper_medium::{Hz, ReaperStr};

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use std::sync::{Arc, OnceLock};

use crate::infrastructure::plugin::backbone_shell::BackboneShell;

use crate::infrastructure::plugin::unit_editor::UnitEditor;
use crate::infrastructure::plugin::unit_shell::UnitShell;
use crate::infrastructure::ui::unit_panel::UnitPanel;
use anyhow::{anyhow, Context};
use helgoboss_learn::AbstractTimestamp;
use std::convert::TryInto;
use std::ptr::null_mut;
use std::rc::Rc;
use std::slice;
use swell_ui::SharedView;
use vst::api::{Events, Supported};
use vst::buffer::AudioBuffer;
use vst::host::Host;

/// Just the old term as alias for easier class search.
type _RealearnPlugin = UnitVstPlugin;

/// The actual VST plug-in and thus main entry point.
///
/// Owns the unit shell, but not immediately. It's created as soon as the containing FX is
/// available.
pub struct UnitVstPlugin {
    unit_id: UnitId,
    host: HostCallback,
    unit_parameters: Arc<UnitParameterContainer>,
    /// This will be set on `init()`.
    _reaper_guard: Option<Arc<ReaperGuard>>,
    // For detecting play state changes
    was_playing_in_last_cycle: bool,
    sample_rate: Hz,
    /// **After `init` has been called**, this should be `true` if the plug-in was loaded by the
    /// REAPER VST scan (and is not going to be used "for real").
    is_plugin_scan: bool,
    // This will be set as soon as the containing FX is known (one main loop cycle after `init()`).
    unit_shell: OnceLock<Arc<UnitShell>>,
    unit_panel: SharedView<UnitPanel>,
}

impl Default for UnitVstPlugin {
    fn default() -> Self {
        UnitVstPlugin::new(Default::default())
    }
}

unsafe impl Send for UnitVstPlugin {}

impl Plugin for UnitVstPlugin {
    fn new(host: HostCallback) -> Self {
        firewall(|| {
            let unit_parameters = Arc::new(UnitParameterContainer::new());
            Self {
                unit_id: UnitId::next(),
                host,
                _reaper_guard: None,
                unit_parameters,
                was_playing_in_last_cycle: false,
                sample_rate: Default::default(),
                is_plugin_scan: false,
                unit_shell: OnceLock::new(),
                unit_panel: Rc::new(UnitPanel::new()),
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
        let params = self.unit_parameters.params();
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
            // Trick to find out whether we are only being scanned.
            self.is_plugin_scan = unsafe { (*self.host.raw_effect()).reserved1 == 0 };
            if self.is_plugin_scan {
                tracing_debug!("ReaLearn is being scanned by REAPER");
                return;
            }
            tracing_debug!("ReaLearn is being opened by REAPER");
            self._reaper_guard = Some(self.ensure_reaper_setup());
            // At this point, REAPER cannot reliably give us the containing FX. As a
            // consequence we also don't have a unit shell yet, because creating an incomplete
            // unit shell pushes the problem of not knowing the containing FX into the application
            // logic, which we for sure don't want. In the next main loop cycle, it should be
            // possible to identify the containing FX.
            let host = self.host;
            Global::task_support()
                .do_later_in_main_thread_from_main_thread_asap(move || {
                    let plugin = unsafe { (*host.raw_effect()).get_plugin() };
                    plugin.vendor_specific(INIT_FIRST_INSTANCE_VENDOR_CODE, 0, null_mut(), 0.0);
                })
                .unwrap();
        });
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        firewall(|| {
            // Unfortunately, vst-rs calls `get_editor` before the plug-in is initialized by the
            // host, e.g. in order to check if it should the hasEditor flag or not. That means
            // we don't know yet if this is a plug-in scan or not. We have to create the editor.
            let boxed: Box<dyn Editor> = Box::new(UnitEditor::new(self.unit_panel.clone()));
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
        self.unit_parameters.clone()
    }

    fn vendor_specific(&mut self, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
        firewall(|| {
            // tracing_debug!("VST vendor specific (index = {})", index);
            self.handle_vendor_specific(index, value, ptr, opt)
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
                    if let Some(unit_shell) = self.unit_shell.get() {
                        unit_shell.process_incoming_midi_from_plugin(
                            our_event,
                            is_transport_start,
                            self.host,
                        );
                    }
                }
            });
        });
    }

    fn process_f64(&mut self, buffer: &mut AudioBuffer<f64>) {
        #[cfg(not(feature = "playtime"))]
        let _ = buffer;
        firewall(|| {
            assert_no_alloc(|| {
                // Get current time information so we can detect changes in play state reliably
                // (TimeInfoFlags::TRANSPORT_CHANGED doesn't work the way we want it).
                self.was_playing_in_last_cycle = self.is_now_playing();
                if let Some(unit_shell) = self.unit_shell.get() {
                    unit_shell.run_from_plugin(
                        #[cfg(feature = "playtime")]
                        buffer,
                        #[cfg(feature = "playtime")]
                        crate::domain::AudioBlockProps::from_vst(buffer, self.sample_rate),
                        self.host,
                    );
                }
            });
        });
    }

    fn set_sample_rate(&mut self, rate: f32) {
        firewall(|| {
            tracing_debug!("VST set sample rate");
            self.sample_rate = Hz::new(rate as _);
            if let Some(unit_shell) = self.unit_shell.get() {
                unit_shell.set_sample_rate(rate);
            }
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

impl UnitVstPlugin {
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
                // If the Helgobox Extension is installed, it already called our eager-loading
                // extension entry point. In this case, the following call will not have any
                // effect. And that's exactly what we want, because the App already has been
                // initialized then!
                BackboneShell::make_available_globally(|| BackboneShell::init(context));
            },
            || {
                BackboneShell::get().wake_up();
                || {
                    BackboneShell::get().go_to_sleep();
                }
            },
        )
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
            // TODO-high CONTINUE Rename to HELGOBOX_UNIT_ID. Or no. Swap instance and unit.
            //  It's better if the container is named "instance" because it corresponds to one
            //  plug-in instance. Then we can use ReaLearnUnit, PotUnit, and PlaytimeUnit (Matrix).
            crate::domain::HELGOBOX_INSTANCE_ID => {
                let instance_id_c_string =
                    CString::new(self.unit_id.to_string()).expect("should be number");
                let mut bytes = instance_id_c_string
                    .as_bytes_with_nul()
                    .iter()
                    .map(|v| *v as i8);
                buffer[0..bytes.len()].fill_with(|| bytes.next().unwrap());
                Ok(())
            }
            _ => Err("unhandled config param"),
        }
    }

    fn set_named_config_param(
        &self,
        param_name: &str,
        buffer: *const c_char,
    ) -> anyhow::Result<()> {
        match param_name {
            SET_STATE_PARAM_NAME => {
                let c_str = unsafe { CStr::from_ptr(buffer) };
                let rust_str = c_str.to_str().expect("not valid UTF-8");
                self.unit_parameters.load_state(rust_str)?;
                Ok(())
            }
            _ => Err(anyhow!("unhandled config param")),
        }
    }

    fn init_unit_shell(&self) -> anyhow::Result<()> {
        let processor_context = ProcessorContext::from_host(self.host)
            .context("couldn't build processor context, called too early.")?;
        let unit_shell = Arc::new(UnitShell::new(
            processor_context,
            self.unit_parameters.clone(),
            self.unit_panel.clone(),
        ));
        unit_shell.set_sample_rate(self.sample_rate.get() as _);
        self.unit_shell
            .set(unit_shell.clone())
            .map_err(|_| anyhow!("unit shell already initialized"))?;
        self.unit_parameters
            .notify_unit_shell_is_available(unit_shell);
        Ok(())
    }

    fn handle_vendor_specific(
        &mut self,
        index: i32,
        value: isize,
        ptr: *mut c_void,
        opt: f32,
    ) -> isize {
        use plugin::OpCode::*;
        fn interpret_as_param_name(value: isize) -> Result<&'static str, &'static str> {
            let param_name = unsafe { CStr::from_ptr(value as *const c_char) };
            param_name.to_str().map_err(|_| "invalid parameter name")
        }
        match index {
            INIT_FIRST_INSTANCE_VENDOR_CODE => {
                self.init_unit_shell().expect("couldn't init unit shell");
                return 0;
            }
            _ => {}
        }
        let cockos_opcode: plugin::OpCode = match index.try_into() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        match cockos_opcode {
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
                let params = self.unit_parameters.params();
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
                let params = self.unit_parameters.params();
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

fn firewall<F: FnOnce() -> R, R>(f: F) -> Option<R> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

/// This is our own code. We call ourselves in order to safe us an Arc around
/// the unit shell. Why use an Arc (and therefore make each internal access to the unit shell have to
/// dereference a pointer) if we already have a pointer from outside.
const INIT_FIRST_INSTANCE_VENDOR_CODE: i32 = -235978234;
