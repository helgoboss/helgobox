#[cfg(target_os = "linux")]
pub mod bindings_linux;
#[cfg(target_os = "linux")]
pub use bindings_linux as bindings;

#[cfg(target_os = "windows")]
pub mod bindings_windows;
#[cfg(target_os = "windows")]
pub use bindings_windows as bindings;

pub mod win32;
