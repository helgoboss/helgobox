use libloading::{Library, Symbol};
use winapi::shared::minwindef::UINT;
use winapi::shared::windef::HWND;

/// Provides access to some Win32 API functions that are not available in older Windows versions.
///
/// This is better than eagerly linking to these functions because then the resulting binary
/// wouldn't work *at all* in the older Windows versions, whereas with this approach, we can
/// fall back to alternative logic or alternative values on a case-by-case basis.  
pub struct DynamicWinApi {
    user32_library: Library,
}

impl DynamicWinApi {
    pub fn load() -> Self {
        unsafe {
            Self {
                user32_library: Library::new("user32.dll").unwrap(),
            }
        }
    }

    /// Should be available from Windows 10 onwards.
    pub fn get_dpi_for_window(&self) -> Option<Symbol<GetDpiForWindow>> {
        unsafe { self.user32_library.get(b"GetDpiForWindow\0").ok() }
    }
}

type GetDpiForWindow = extern "system" fn(hwnd: HWND) -> UINT;
