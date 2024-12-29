use vst::editor::Editor;
use vst::plugin;
use vst::plugin::{
    CanDo, Category, HostCallback, Info, Plugin, PluginParameterCharacter, PluginParameterInfo,
    PluginParameters,
};

use crate::domain::{
    ControlEvent, ControlEventTimestamp, InstanceId, MidiEvent, ParameterManager, PluginParamIndex,
    ProcessorContext, RawParamValue, GLOBAL_AUDIO_STATE, PLUGIN_PARAMETER_COUNT,
};
use crate::infrastructure::plugin::instance_parameter_container::InstanceParameterContainer;
use crate::infrastructure::plugin::{init_backbone_shell, SET_STATE_PARAM_NAME};
use base::Global;
use helgobox_allocator::*;
use reaper_high::{Reaper, ReaperGuard};
use reaper_low::{static_plugin_context, PluginContext};
use reaper_medium::{Hz, ReaperStr};

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};

use std::sync::{Arc, OnceLock};

use crate::infrastructure::plugin::backbone_shell::BackboneShell;

use crate::infrastructure::data::InstanceData;
use crate::infrastructure::plugin::helgobox_plugin_editor::HelgoboxPluginEditor;
use crate::infrastructure::plugin::instance_shell::InstanceShell;
use crate::infrastructure::ui::instance_panel::InstancePanel;
use anyhow::{anyhow, Context};
use std::convert::TryInto;
use std::ptr::null_mut;
use std::rc::Rc;
use std::slice;
use swell_ui::SharedView;
use vst::api::{Events, Supported};
use vst::buffer::AudioBuffer;
use vst::host::Host;

/// Just the old term as alias for easier class search.
type _RealearnPlugin = HelgoboxPlugin;

/// In C++ this is the same like "hbrl" (= Helgoboss ReaLearn)
pub const HELGOBOX_UNIQUE_VST_PLUGIN_ID: i32 = 1751282284;

/// Can be passed to `add_fx_by_original_name`.
pub const HELGOBOX_UNIQUE_VST_PLUGIN_ADD_STRING: &str = "<1751282284";

/// The actual VST plug-in and thus main entry point.
///
/// Owns the instance shell, but not immediately. It's created as soon as the containing FX is
/// available.
pub struct HelgoboxPlugin {
    instance_id: InstanceId,
    host: HostCallback,
    param_container: Arc<InstanceParameterContainer>,
    // For detecting play state changes
    was_playing_in_last_cycle: bool,
    /// I think this sample rate can be different from the device sample rate in some cases (oversampling).
    sample_rate: Hz,
    block_size: u32,
    /// **After `init` has been called**, this should be `true` if the plug-in was loaded by the
    /// REAPER VST scan (and is not going to be used "for real").
    is_plugin_scan: bool,
    // This will be set as soon as the containing FX is known (one main loop cycle after `init()`).
    lazy_data: OnceLock<LazyData>,
    instance_panel: SharedView<InstancePanel>,
    /// This will be set on `init()`.
    ///
    /// This should be the last because the other members should be dropped first (especially lazy
    /// data including instance shell)!
    _reaper_guard: Option<Arc<ReaperGuard>>,
}

impl Drop for HelgoboxPlugin {
    fn drop(&mut self) {
        tracing::debug!("Dropping HelgoboxPlugin");
    }
}

#[derive(Clone)]
struct LazyData {
    instance_shell: Arc<InstanceShell>,
    /// Only the parameters of the main unit are exposed as VST parameters.
    main_unit_parameter_manager: Arc<ParameterManager>,
}

impl Default for HelgoboxPlugin {
    fn default() -> Self {
        HelgoboxPlugin::new(Default::default())
    }
}

unsafe impl Send for HelgoboxPlugin {}

impl Plugin for HelgoboxPlugin {
    fn new(host: HostCallback) -> Self {
        let instance_id = InstanceId::next();
        Self {
            instance_id,
            host,
            _reaper_guard: None,
            param_container: Arc::new(InstanceParameterContainer::new()),
            was_playing_in_last_cycle: false,
            sample_rate: Default::default(),
            block_size: 0,
            is_plugin_scan: false,
            lazy_data: OnceLock::new(),
            instance_panel: Rc::new(InstancePanel::new(instance_id)),
        }
    }

    fn get_info(&self) -> Info {
        Info {
            name: "Helgobox - ReaLearn & Playtime".to_string(),
            vendor: "Helgoboss".to_string(),
            unique_id: HELGOBOX_UNIQUE_VST_PLUGIN_ID,
            preset_chunks: true,
            category: Category::Synth,
            parameters: PLUGIN_PARAMETER_COUNT as i32,
            f64_precision: true,
            inputs: 2,
            outputs: 0,
            ..Default::default()
        }
    }

    fn get_parameter_info(&self, index: i32) -> Option<PluginParameterInfo> {
        let i = PluginParamIndex::try_from(index as u32).ok()?;
        let params = self.lazy_data.get()?.main_unit_parameter_manager.params();
        let param = params.at(i);
        let value_count = param.setting().value_count?;
        let mut info = PluginParameterInfo::default();
        info.character = PluginParameterCharacter::Discrete {
            min: 0,
            max: (value_count.get() - 1) as i32,
            steps: None,
        };
        Some(info)
    }

    fn init(&mut self) {
        // Trick to find out whether we are only being scanned.
        self.is_plugin_scan = unsafe { (*self.host.raw_effect()).reserved1 == 0 };
        if self.is_plugin_scan {
            tracing::debug!("Helgobox is being scanned by REAPER");
            return;
        }
        tracing::debug!("Helgobox is being opened by REAPER");
        self._reaper_guard = Some(self.ensure_reaper_setup());
        // At this point, REAPER cannot reliably give us the containing FX. As a
        // consequence we also don't have a instance shell yet, because creating an incomplete
        // instance shell pushes the problem of not knowing the containing FX into the application
        // logic, which we for sure don't want. In the next main loop cycle, it should be
        // possible to identify the containing FX.
        let host = self.host;
        Global::task_support()
            .do_later_in_main_thread_from_main_thread_asap(move || {
                let plugin = unsafe { (*host.raw_effect()).get_plugin() };
                plugin.vendor_specific(INIT_INSTANCE_SHELL, 0, null_mut(), 0.0);
            })
            .unwrap();
    }

    fn get_editor(&mut self) -> Option<Box<dyn Editor>> {
        // Unfortunately, vst-rs calls `get_editor` before the plug-in is initialized by the
        // host, e.g. in order to check if it should the hasEditor flag or not. That means
        // we don't know yet if this is a plug-in scan or not. We have to create the editor.
        let boxed: Box<dyn Editor> =
            Box::new(HelgoboxPluginEditor::new(self.instance_panel.clone()));
        Some(boxed)
    }

    fn can_do(&self, can_do: CanDo) -> Supported {
        use CanDo::*;
        use Supported::*;
        #[allow(overflowing_literals)]
        match can_do {
            SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent | ReceiveSysExEvent => {
                Supported::Yes
            }
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
    }

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        self.param_container.clone()
    }

    fn vendor_specific(&mut self, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
        // tracing_debug!("VST vendor specific (index = {})", index);
        self.handle_vendor_specific(index, value, ptr, opt)
    }

    fn process_events(&mut self, events: &Events) {
        assert_no_alloc(|| {
            let is_transport_start = !self.was_playing_in_last_cycle && self.is_now_playing();
            let block_count = GLOBAL_AUDIO_STATE.load_block_count();
            let sample_count = block_count * self.block_size as u64;
            let device_sample_rate = GLOBAL_AUDIO_STATE.load_sample_rate();
            for e in events.events() {
                let our_event = match MidiEvent::from_vst(e) {
                    Err(_) => {
                        // Just ignore if not a valid MIDI message. Invalid MIDI message was
                        // observed in the wild: https://github.com/helgoboss/helgobox/issues/82.
                        continue;
                    }
                    Ok(e) => e,
                };
                let timestamp = ControlEventTimestamp::from_rt(
                    sample_count,
                    device_sample_rate,
                    our_event.offset().to_seconds(self.sample_rate),
                );
                let our_event = ControlEvent::new(our_event, timestamp);
                if let Some(lazy_data) = self.lazy_data.get() {
                    lazy_data.instance_shell.process_incoming_midi_from_plugin(
                        our_event,
                        is_transport_start,
                        self.host,
                    );
                }
            }
        });
    }

    fn process_f64(&mut self, buffer: &mut AudioBuffer<f64>) {
        assert_no_alloc(|| {
            // Get current time information so we can detect changes in play state reliably
            // (TimeInfoFlags::TRANSPORT_CHANGED doesn't work the way we want it).
            self.was_playing_in_last_cycle = self.is_now_playing();
            if let Some(lazy_data) = self.lazy_data.get() {
                #[cfg(feature = "playtime")]
                lazy_data.instance_shell.run_playtime_from_plugin(
                    buffer,
                    crate::domain::AudioBlockProps::from_vst(buffer, self.sample_rate),
                );
                lazy_data.instance_shell.run_from_plugin(self.host);
            }
        });
        let _ = buffer;
    }

    fn set_sample_rate(&mut self, rate: f32) {
        tracing::debug!("VST set sample rate");
        self.sample_rate = Hz::new_panic(rate as _);
        if let Some(lazy_data) = self.lazy_data.get() {
            lazy_data.instance_shell.set_sample_rate(rate);
        }
    }

    fn suspend(&mut self) {
        tracing::debug!("VST suspend");
    }

    fn resume(&mut self) {
        tracing::debug!("VST resume");
    }

    fn set_block_size(&mut self, size: i64) {
        tracing::debug!("VST set block size");
        self.block_size = size as u32;
    }

    fn start_process(&mut self) {
        tracing::debug!("VST start process");
    }

    fn stop_process(&mut self) {
        tracing::debug!("VST stop process");
    }
}

impl HelgoboxPlugin {
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
            // We let reaper-rs wake up and go to sleep in Backbone init/drop in order to let it be awake even
            // if not VST plug-in instance is around
            false,
            || {
                let context =
                    PluginContext::from_vst_plugin(&self.host, static_plugin_context()).unwrap();
                // If the Helgobox Extension is installed, it already called our eager-loading
                // extension entry point. In this case, the following call will not have any
                // effect. And that's exactly what we want, because the App already has been
                // initialized then!
                init_backbone_shell(context);
            },
            || {
                let _ = BackboneShell::get().wake_up();
                || {
                    BackboneShell::get().go_to_sleep();
                }
            },
        )
    }

    fn get_lazy_data(&self) -> Result<&LazyData, &'static str> {
        self.lazy_data.get().ok_or("lazy data not available yet")
    }

    fn get_parameter_display(&self, index: u32, raw_value: f32) -> Result<String, &'static str> {
        let i = PluginParamIndex::try_from(index)?;
        let display = self
            .get_lazy_data()?
            .main_unit_parameter_manager
            .params()
            .at(i)
            .setting()
            .with_raw_value(raw_value)
            .to_string();
        Ok(display)
    }

    /// Returns `None` if REAPER string is empty (REAPER's way of checking whether
    /// we support this).
    fn string_to_parameter(
        &self,
        index: u32,
        reaper_str: &ReaperStr,
    ) -> Result<Option<RawParamValue>, &'static str> {
        let text_input = reaper_str.to_str();
        if text_input.is_empty() {
            // REAPER checks if we support this.
            return Ok(None);
        }
        let i = PluginParamIndex::try_from(index)?;
        let params = self.get_lazy_data()?.main_unit_parameter_manager.params();
        let param = params.at(i);
        let raw_value = param.setting().parse_to_raw_value(text_input)?;
        Ok(Some(raw_value))
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
            crate::domain::HELGOBOX_INSTANCE_ID_KEY => {
                let instance_id_c_string =
                    CString::new(self.instance_id.to_string()).expect("should be number");
                let mut bytes = instance_id_c_string
                    .as_bytes_with_nul()
                    .iter()
                    .map(|v| *v as c_char);
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
                let data = InstanceData::parse(c_str.to_bytes())?;
                let lazy_data = self.lazy_data.get().context("lazy data not yet set")?;
                lazy_data.instance_shell.clone().apply_data(data)?;
                Ok(())
            }
            _ => Err(anyhow!("unhandled config param")),
        }
    }

    fn init_instance_shell(&self) -> anyhow::Result<()> {
        let processor_context = ProcessorContext::from_host(self.host)
            .context("couldn't build processor context, called too early.")?;
        let instance_shell = InstanceShell::new(
            self.instance_id,
            processor_context,
            self.instance_panel.clone(),
        );
        let instance_shell = Arc::new(instance_shell);
        BackboneShell::get().register_instance(&instance_shell);
        self.instance_panel
            .notify_shell_available(instance_shell.clone());
        instance_shell.set_sample_rate(self.sample_rate.get() as _);
        let main_unit_parameter_manager = instance_shell
            .main_unit_shell()
            .model()
            .borrow()
            .unit()
            .borrow()
            .parameter_manager()
            .clone();
        let lazy_data = LazyData {
            instance_shell,
            main_unit_parameter_manager,
        };
        self.lazy_data
            .set(lazy_data.clone())
            .map_err(|_| anyhow!("lazy data already initialized"))?;
        self.param_container.notify_instance_shell_available(
            &lazy_data.instance_shell,
            lazy_data.main_unit_parameter_manager,
        );
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
            INIT_INSTANCE_SHELL => {
                self.init_instance_shell()
                    .expect("couldn't init instance shell");
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
                let Ok(display) = self.get_parameter_display(value as u32, opt) else {
                    return 0;
                };
                if write_to_c_str(ptr, display).is_err() {
                    return 0;
                }
                0xbeef
            }
            // Cockos: Parse parameter value without setting it (http://reaper.fm/sdk/vst/vst_ext.php)
            StringToParameter if !ptr.is_null() && value >= 0 => {
                let reaper_str = unsafe { ReaperStr::from_ptr(ptr as *const c_char) };
                let Ok(raw_value) = self.string_to_parameter(value as u32, reaper_str) else {
                    return 0;
                };
                let Some(raw_value) = raw_value else {
                    // REAPER checks if we support this.
                    return 0xbeef;
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

/// This is our own code. We call ourselves in order to safe us an Arc around
/// the instance shell. Why use an Arc (and therefore make each internal access to the instance shell have to
/// dereference a pointer) if we already have a pointer from outside.
const INIT_INSTANCE_SHELL: i32 = -235978234;
