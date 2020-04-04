mod model;
mod plugin;
mod view;

use crate::view::GLOBAL_HINSTANCE;
use plugin::RealearnPlugin;
use std::panic::catch_unwind;
use std::sync::Once;
use vst::plugin_main;
use winapi::_core::ptr::null_mut;

static INIT_GLOBAL_HINSTANCE: Once = Once::new();

// This is for getting a reference to the DLL's HMODULE/HINSTANCE address, which is necessary to
// access the dialog resources for the Win32 UI.
#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
#[no_mangle]
extern "system" fn DllMain(hinstance: *const u8, _: u32, _: *const u8) -> u32 {
    INIT_GLOBAL_HINSTANCE.call_once(|| {
        unsafe { GLOBAL_HINSTANCE = hinstance as *mut winapi::shared::minwindef::HINSTANCE__ };
    });
    1
}

// plugin_main!(RealearnPlugin);

#[cfg(target_os = "macos")]
#[no_mangle]
pub extern "system" fn main_macho(callback: vst::api::HostCallbackProc) -> *mut vst::api::AEffect {
    VSTPluginMain(callback)
}
#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
#[no_mangle]
pub extern "system" fn MAIN(callback: vst::api::HostCallbackProc) -> *mut vst::api::AEffect {
    VSTPluginMain(callback)
}
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn VSTPluginMain(callback: vst::api::HostCallbackProc) -> *mut vst::api::AEffect {
    catch_unwind(|| vst::main::<RealearnPlugin>(callback)).unwrap_or(null_mut())
}
