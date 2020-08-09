use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Generate "built" file (containing build-time information)
    built::write_built_file().expect("Failed to acquire build-time information");

    // Optionally generate bindings
    #[cfg(feature = "generate")]
    generate_bindings();

    // Embed or compile dialogs
    #[cfg(target_family = "windows")]
    embed_dialog_resources();
    #[cfg(target_family = "unix")]
    compile_dialogs();

    // Compile WDL EEL
    compile_eel();
}

fn compile_eel() {
    let asm_object_file = if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            Some("lib/WDL/WDL/eel2/asm-nseel-x64.obj")
        } else {
            None
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "x86_64") {
            Some("lib/WDL/WDL/eel2/asm-nseel-x64-macho.o")
        } else {
            None
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            // Generate asm-nseel-x64.o
            Command::new("make")
                .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib/WDL/WDL/eel2"))
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
        .file("lib/WDL/WDL/eel2/nseel-cfunc.c")
        .file("lib/WDL/WDL/eel2/nseel-compiler.c")
        .file("lib/WDL/WDL/eel2/nseel-caltab.c")
        .file("lib/WDL/WDL/eel2/nseel-eval.c")
        .file("lib/WDL/WDL/eel2/nseel-lextab.c")
        .file("lib/WDL/WDL/eel2/nseel-ram.c")
        .file("lib/WDL/WDL/eel2/nseel-yylex.c");
    if let Some(f) = asm_object_file {
        build.object(f);
    }
    build.compile("wdl-eel");
}

/// Compiles dialog windows using SWELL's dialog generator (too obscure to be ported to Rust)
#[cfg(target_family = "unix")]
fn compile_dialogs() {
    // Make RC file SWELL-compatible.
    // ResEdit uses WS_CHILDWINDOW but SWELL understands WS_CHILD only. Rename it.
    let modified_rc_content = std::fs::read_to_string("src/infrastructure/common/realearn.rc")
        .expect("couldn't read RC file")
        .replace("WS_CHILDWINDOW", "WS_CHILD");
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
        "src/infrastructure/common/realearn.rc_mac_dlg",
    )
    .unwrap();
    assert!(result.status.success(), "PHP dialog translator failed");
    // Compile the resulting C++ file
    cc::Build::new()
        .cpp(true)
        .cpp_set_stdlib(determine_cpp_stdlib())
        .warnings(false)
        .file("src/infrastructure/common/dialogs.cpp")
        .compile("dialogs");
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
    embed_resource::compile("src/infrastructure/common/realearn.rc");
}

/// Generates Rust bindings for a couple of C stuff.
#[cfg(feature = "generate")]
fn generate_bindings() {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=src/infrastructure/common/wrapper.hpp");
    let mut builder = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("src/infrastructure/common/wrapper.hpp")
        // .opaque_type("timex")
        // .derive_eq(true)
        // .derive_partialeq(true)
        // .derive_hash(true)
        .clang_arg("-xc++")
        .enable_cxx_namespaces()
        .raw_line("#![allow(non_upper_case_globals)]")
        .raw_line("#![allow(non_camel_case_types)]")
        .raw_line("#![allow(non_snake_case)]")
        .raw_line("#![allow(dead_code)]")
        // ReaLearn UI
        .whitelist_var("ID_.*")
        .whitelist_function("NSEEL_.*")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks));
    if let Some(stdlib) = determine_cpp_stdlib() {
        builder = builder.clang_arg(format!("-stdlib=lib{}", stdlib));
    }
    let bindings = builder
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");
    // Write the bindings to the bindings.rs file.
    let out_path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    bindings
        .write_to_file(
            out_path
                .join("src/infrastructure/common")
                .join("bindings.rs"),
        )
        .expect("Couldn't write bindings!");
}

#[cfg(any(feature = "generate", target_family = "unix"))]
fn determine_cpp_stdlib() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("c++")
    } else {
        None
    }
}
