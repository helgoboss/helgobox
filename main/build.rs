fn main() {
    // Generate "built" file (containing build-time information)
    built::write_built_file().expect("Failed to acquire build-time information");

    // Optionally generate bindings and dialogs
    #[cfg(feature = "generate")]
    codegen::generate_bindings();
    #[cfg(all(feature = "generate", target_family = "unix"))]
    codegen::generate_dialogs();

    // Embed or compile dialogs
    #[cfg(target_family = "windows")]
    embed_dialog_resources();
    #[cfg(target_family = "unix")]
    compile_dialogs();

    // Compile WDL EEL
    compile_eel();
}

fn compile_eel() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let asm_object_file = if cfg!(target_os = "windows") {
        if target_arch == "x86_64" {
            Some("lib/WDL/WDL/eel2/asm-nseel-x64.obj")
        } else {
            None
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
                .arg("asm-nseel-x64.o")
                .output()
                .expect("Failed to generate asm-nseel-x64.o. Maybe 'nasm' is not installed.");
            Some("lib/WDL/WDL/eel2/asm-nseel-x64.o")
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
        .file("src/infrastructure/ui/dialogs.cpp");
    if let Some(stdlib) = util::determine_cpp_stdlib() {
        // Settings this to None on Linux causes the linker to automatically link against C++
        // anymore, so we just invoke that on macOS.
        build.cpp_set_stdlib(stdlib);
    }
    build.compile("dialogs");
}

/// On Windows we can directly embed the dialog resource file produced by ResEdit.
#[cfg(target_family = "windows")]
fn embed_dialog_resources() {
    let target = std::env::var("TARGET").unwrap();
    if let Some(tool) = cc::windows_registry::find_tool(target.as_str(), "cl.exe") {
        for (key, value) in tool.env() {
            std::env::set_var(key, value);
        }
    }
    embed_resource::compile("src/infrastructure/ui/msvc/msvc.rc");
}

#[cfg(feature = "generate")]
mod codegen {
    use crate::util;

    /// Generates Rust bindings for a couple of C stuff.
    pub fn generate_bindings() {
        generate_core_bindings();
        generate_infrastructure_bindings();
    }

    /// Generates dialog window C++ code from resource file using SWELL's PHP-based dialog generator
    /// (too obscure to be ported to Rust).
    #[cfg(target_family = "unix")]
    pub fn generate_dialogs() {
        use std::io::Read;
        // Make RC file SWELL-compatible.
        // ResEdit uses WS_CHILDWINDOW but SWELL understands WS_CHILD only. Rename it.
        let mut rc_file = std::fs::File::open("src/infrastructure/ui/msvc/msvc.rc")
            .expect("couldn't find msvc.rc");
        let mut rc_buf = vec![];
        rc_file
            .read_to_end(&mut rc_buf)
            .expect("couldn't read msvc.rc");
        let (original_rc_content, ..) = encoding_rs::UTF_16LE.decode(&rc_buf);
        let modified_rc_content = original_rc_content.replace("WS_CHILDWINDOW", "WS_CHILD");
        std::fs::write("../target/realearn.modified.rc", modified_rc_content)
            .expect("couldn't write modified RC file");
        // Use PHP to translate SWELL-compatible RC file to C++
        let result = std::process::Command::new("php")
            .arg("lib/WDL/WDL/swell/mac_resgen.php")
            .arg("../target/realearn.modified.rc")
            .output()
            .expect("PHP dialog translator result not available");
        std::fs::copy(
            "../target/realearn.modified.rc_mac_dlg",
            "src/infrastructure/ui/realearn.rc_mac_dlg",
        )
        .unwrap();
        std::fs::copy(
            "../target/realearn.modified.rc_mac_menu",
            "src/infrastructure/ui/realearn.rc_mac_menu",
        )
        .unwrap();
        assert!(result.status.success(), "PHP dialog generation failed");
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
            .whitelist_function("NSEEL_.*")
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

    fn generate_infrastructure_bindings() {
        // Tell cargo to invalidate the built crate whenever the wrapper changes
        println!("cargo:rerun-if-changed=src/infrastructure/ui/wrapper.hpp");
        let bindings = bindgen::Builder::default()
            .header("src/infrastructure/ui/wrapper.hpp")
            .whitelist_var("ID_.*")
            .whitelist_var("IDC_.*")
            .whitelist_var("IDM_.*")
            .whitelist_var("IDR_.*")
            .enable_cxx_namespaces()
            .parse_callbacks(Box::new(bindgen::CargoCallbacks))
            .generate()
            .expect("Unable to generate bindings");
        let out_path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("src/infrastructure/ui/bindings.rs"))
            .expect("Couldn't write bindings!");
    }
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
