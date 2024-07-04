use anyhow::{bail, Context, Result};
use libloading::{Library, Symbol};
use reaper_fluent::Reaper;
use reaper_low::{PluginContext, TypeSpecificPluginContext};
use reaper_macros::reaper_extension_plugin;
use reaper_medium::ReaperSession;
use std::error::Error;
use std::fs;
use std::sync::OnceLock;

// Executing Drop not important because extensions always live until REAPER ends.
static EXTENSION: OnceLock<HelgoboxExtension> = OnceLock::new();

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> std::result::Result<(), Box<dyn Error>> {
    let _ = EXTENSION.set(HelgoboxExtension::load(context)?);
    Ok(())
}

type ReaperPluginEntry = unsafe extern "C" fn(
    h_instance: ::reaper_low::raw::HINSTANCE,
    rec: *mut ::reaper_low::raw::reaper_plugin_info_t,
) -> ::std::os::raw::c_int;

struct HelgoboxExtension {
    /// Just for RAII.
    _plugin_library: Option<Library>,
}

impl HelgoboxExtension {
    pub fn load(context: PluginContext) -> Result<Self> {
        // Install reaper-fluent for global use
        let _ = Reaper::install_globally(ReaperSession::load(context));
        // Return extension
        let extension = Self {
            _plugin_library: eagerly_load_plugin_lib(&context).ok(),
        };
        Ok(extension)
    }

    #[allow(dead_code)]
    pub fn get() -> &'static HelgoboxExtension {
        EXTENSION
            .get()
            .expect("Helgobox extension not yet initialized")
    }
}

/// Loads the Helgobox plug-in library eagerly (Justin's idea, awesome!)
fn eagerly_load_plugin_lib(context: &PluginContext) -> Result<Library> {
    // Find correct shared library file
    let resource_path = Reaper::get()
        .medium_reaper()
        .get_resource_path(|path| path.to_path_buf());
    let plugin_library_path = fs::read_dir(resource_path.join("UserPlugins/FX"))?
        .flatten()
        .find_map(|item| {
            let file_type = item.file_type().ok()?;
            if !file_type.is_file() && !file_type.is_symlink() {
                return None;
            }
            let file_name = item.file_name().to_str()?.to_lowercase();
            let extension = if cfg!(target_os = "windows") {
                ".dll"
            } else if cfg!(target_os = "macos") {
                ".vst.dylib"
            } else {
                ".so"
            };
            let matches = file_name.starts_with("helgobox") && file_name.ends_with(extension);
            if !matches {
                return None;
            }
            Some(item.path())
        })
        .context("couldn't find plug-in library")?;
    // Load shared library
    let plugin_library = unsafe { Library::new(plugin_library_path)? };
    // Run extension entry point of library
    let reaper_plugin_entry: Symbol<ReaperPluginEntry> =
        unsafe { plugin_library.get(b"ReaperPluginEntry")? };
    let TypeSpecificPluginContext::Extension(ctx) = context.type_specific() else {
        bail!("unexpected plug-in context type for extension");
    };
    let mut original_info_struct = ctx.to_raw();
    unsafe {
        reaper_plugin_entry(context.h_instance(), &mut original_info_struct as *mut _);
    }
    Ok(plugin_library)
}
