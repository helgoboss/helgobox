use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    // Generate "built" file (containing build-time information)
    built::write_built_file().expect("Failed to acquire build-time information");

    // Generate GUI dialog files (rc file and C header)
    let generated_dir = PathBuf::from("../target/generated");
    let dialog_rc_file = generated_dir.join("msvc.rc");
    generate_gui_dialogs(&generated_dir, &dialog_rc_file)?;

    // Optionally generate bindings (e.g. from Cockos EEL)
    #[cfg(feature = "generate")]
    codegen::generate_bindings();

    // Embed (Windows) or compile (Linux/macOS) dialogs
    #[cfg(target_family = "windows")]
    embed_dialog_resources(&dialog_rc_file);
    #[cfg(target_family = "unix")]
    compile_dialogs();

    // Compile WDL EEL
    compile_eel();

    Ok(())
}

#[allow(unused_variables)]
fn generate_gui_dialogs(
    generated_dir: impl AsRef<Path>,
    dialog_rc_file: impl AsRef<Path>,
) -> Result<(), Box<dyn Error>> {
    let bindings_file = "src/infrastructure/ui/bindings.rs";
    fs::create_dir_all(generated_dir.as_ref())?;
    helgobox_dialogs::generate_dialog_files(generated_dir.as_ref(), bindings_file);
    // On macOS and Linux, try to generate SWELL dialogs (needs PHP)
    #[cfg(target_family = "unix")]
    if let Err(e) = generate_dialogs(dialog_rc_file.as_ref()) {
        println!("cargo:warning={e}");
    }
    Ok(())
}

fn compile_eel() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let asm_object_file = if cfg!(target_os = "windows") {
        match target_arch.as_str() {
            "x86_64" => Some("lib/WDL/WDL/eel2/asm-nseel-x64.obj"),
            "arm64ec" => Some("lib/WDL/WDL/eel2/asm-nseel-arm64ec.obj"),
            _ => None
        }
    } else if cfg!(target_os = "macos") {
        if target_arch == "x86_64" {
            Some("lib/WDL/WDL/eel2/asm-nseel-x64-macho.o")
        } else {
            None
        }
    } else if cfg!(target_os = "linux") {
        if target_arch == "x86_64" {
            // Generate asm-nseel-x64.o
            std::process::Command::new("make")
                .current_dir(
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib/WDL/WDL/eel2"),
                )
                .arg("asm-nseel-x64-sse.o")
                .output()
                .expect("Failed to generate asm-nseel-x64.o. Maybe 'nasm' is not installed.");
            Some("lib/WDL/WDL/eel2/asm-nseel-x64-sse.o")
        } else {
            None
        }
    } else {
        None
    };
    let mut build = cc::Build::new();
    build
        .warnings(false)
        // To make it compile for ARM targets (armv7 and aarch64) whose char type is unsigned.
        .define("WDL_ALLOW_UNSIGNED_DEFAULT_CHAR", None)
        // To make it compile for ARM targets (armv7)
        .define("_FILE_OFFSET_BITS", "64")
        .file("lib/WDL/WDL/eel2/nseel-cfunc.c")
        .file("lib/WDL/WDL/eel2/nseel-compiler.c")
        .file("lib/WDL/WDL/eel2/nseel-caltab.c")
        .file("lib/WDL/WDL/eel2/nseel-eval.c")
        .file("lib/WDL/WDL/eel2/nseel-lextab.c")
        .file("lib/WDL/WDL/eel2/nseel-ram.c")
        .file("lib/WDL/WDL/eel2/nseel-yylex.c");
    if target_arch == "arm" {
        // To make it compile for Linux armv7 targets.
        build.flag_if_supported("-marm");
    }
    if let Some(f) = asm_object_file {
        build.object(f);
    }
    build.compile("wdl-eel");
}

/// Compiles dialog windows code which was previously generated via PHP script.
#[cfg(target_family = "unix")]
fn compile_dialogs() {
    // Compile the C++ file resulting from the PHP script execution in the generate step.
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .warnings(false)
        .file("src/infrastructure/ui/dialogs.cpp")
        // Important when building with Rust 1.61, otherwise missing GUI.
        .link_lib_modifier("+whole-archive");
    if let Some(stdlib) = util::determine_cpp_stdlib() {
        // Settings this to None on Linux causes the linker to automatically link against C++
        // anymore, so we just invoke that on macOS.
        build.cpp_set_stdlib(stdlib);
    }
    build.compile("dialogs");
}

/// On Windows we can directly embed the dialog resource file produced by ResEdit.
#[cfg(target_family = "windows")]
fn embed_dialog_resources(rc_file: impl AsRef<Path>) {
    let target = std::env::var("TARGET").unwrap();
    if let Some(tool) = cc::windows_registry::find_tool(target.as_str(), "cl.exe") {
        for (key, value) in tool.env() {
            std::env::set_var(key, value);
        }
    }
    embed_resource::compile(rc_file, embed_resource::NONE);
}

#[cfg(feature = "generate")]
mod codegen {
    use crate::util;
    use std::error::Error;

    /// Generates Rust bindings for a couple of C stuff.
    pub fn generate_bindings() {
        generate_core_bindings();
    }

    fn generate_core_bindings() {
        println!("cargo:rerun-if-changed=src/base/wrapper.hpp");
        let mut builder = bindgen::Builder::default()
            .header("src/base/wrapper.hpp")
            .clang_arg("-xc++")
            .enable_cxx_namespaces()
            .raw_line("#![allow(non_upper_case_globals)]")
            .raw_line("#![allow(non_camel_case_types)]")
            .raw_line("#![allow(non_snake_case)]")
            .raw_line("#![allow(dead_code)]")
            .raw_line("#![allow(deref_nullptr)]")
            .allowlist_function("NSEEL_.*")
            .allowlist_var("NSEEL_.*")
            .parse_callbacks(Box::new(bindgen::CargoCallbacks));
        if let Some(stdlib) = util::determine_cpp_stdlib() {
            builder = builder.clang_arg(format!("-stdlib=lib{}", stdlib));
        }
        let bindings = builder.generate().expect("Unable to generate bindings");
        let out_path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("src/base/bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}

/// Generates dialog window C++ code from resource file using SWELL's PHP-based dialog generator
/// (too obscure to be ported to Rust).
///
/// # Errors
///
/// Returns an error if PHP is not installed.
#[cfg(target_family = "unix")]
pub fn generate_dialogs(rc_file: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
    // Use PHP to translate SWELL-compatible RC file to C++
    let result = std::process::Command::new("php")
        .arg("lib/WDL/WDL/swell/swell_resgen.php")
        .arg(rc_file.as_ref())
        .output()
        .map_err(|_| {
            "PHP not available, is necessary on macOS and Linux to generate GUI dialogs"
        })?;
    if !result.status.success() {
        panic!("PHP dialog generation failed (PHP available but script failed)");
    }
    Ok(())
}

mod util {
    #[cfg(any(feature = "generate", target_family = "unix"))]
    pub fn determine_cpp_stdlib() -> Option<&'static str> {
        if cfg!(target_os = "macos") {
            Some("c++")
        } else {
            None
        }
    }
}
