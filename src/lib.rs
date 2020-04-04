mod bindings;
mod editor;
mod model;
mod realearn;
mod view;

use crate::editor::GLOBAL_HINSTANCE;
use realearn::Realearn;
use std::sync::Once;
use vst::plugin_main;

plugin_main!(Realearn);

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
