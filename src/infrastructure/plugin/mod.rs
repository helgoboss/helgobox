mod realearn_plugin;

mod realearn_editor;
use realearn_editor::*;

use realearn_plugin::RealearnPlugin;

use vst::plugin_main;

plugin_main!(RealearnPlugin);

#[cfg(target_os = "linux")]
pub(in crate::infrastructure) mod hinstance {
    pub fn get_global_hinstance() -> HINSTANCE {
        null_mut()
    }
}

#[cfg(target_os = "windows")]
pub(in crate::infrastructure) mod hinstance {
    use std::ptr::null_mut;
    use std::sync::Once;
    use winapi::shared::minwindef::HINSTANCE;

    /// On Windows, this returns the DLL's HMODULE/HINSTANCE address as soon as the DLL is loaded,
    /// otherwise null.
    pub fn get_global_hinstance() -> HINSTANCE {
        unsafe { HINSTANCE }
    }

    static mut HINSTANCE: HINSTANCE = null_mut();
    static INIT_HINSTANCE: Once = Once::new();

    /// This is for getting a reference to the DLL's HMODULE/HINSTANCE address, which is necessary
    /// to access the dialog resources for the Win32 UI.
    #[allow(non_snake_case)]
    #[no_mangle]
    extern "system" fn DllMain(hinstance: *const u8, _: u32, _: *const u8) -> u32 {
        INIT_HINSTANCE.call_once(|| {
            unsafe { HINSTANCE = hinstance as HINSTANCE };
        });
        1
    }
}
