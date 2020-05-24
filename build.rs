fn main() {
    // Bindings
    #[cfg(target_os = "linux")]
    #[cfg(feature = "generate")]
    generate_bindings();

    // Scrollbar library
    compile_coolscroll();

    // Dialogs
    #[cfg(target_os = "windows")]
    embed_dialog_resources();
    #[cfg(not(target_os = "windows"))]
    compile_dialogs();
}

fn compile_coolscroll() {
    cc::Build::new()
        .cpp(true)
        .warnings(false)
        .file("lib/WDL/WDL/wingui/scrollbar/coolscroll.cpp")
        .file("lib/WDL/WDL/lice/lice.cpp")
        .compile("coolscroll");
}

/// Compiles dialog windows using SWELL's dialog generator (too obscure to be ported to Rust)
#[cfg(not(target_os = "windows"))]
fn compile_dialogs() {
    // Make RC file SWELL-compatible.
    // ResEdit uses WS_CHILDWINDOW but SWELL understands WS_CHILD only. Rename it.
    let mut modified_rc_content = std::fs::read_to_string("src/infrastructure/common/realearn.rc")
        .expect("couldn't read RC file")
        .replace("WS_CHILDWINDOW", "WS_CHILD");
    std::fs::write("target/realearn.modified.rc", modified_rc_content)
        .expect("couldn't write modified RC file");
    // Use PHP to translate SWELL-compatible RC file to C++
    let result = std::process::Command::new("php")
        .arg("lib/WDL/WDL/swell/mac_resgen.php")
        .arg("target/realearn.modified.rc")
        .output()
        .expect("PHP dialog translator result not available");
    std::fs::copy(
        "target/realearn.modified.rc_mac_dlg",
        "src/infrastructure/common/realearn.rc_mac_dlg",
    );
    assert!(result.status.success(), "PHP dialog translator failed");
    // Compile the resulting C++ file
    cc::Build::new()
        .cpp(true)
        .warnings(false)
        .file("src/infrastructure/common/dialogs.cpp")
        .compile("dialogs");
}

/// On Windows we can directly embed the dialog resource file produced by ResEdit.
#[cfg(target_os = "windows")]
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
#[cfg(target_os = "linux")]
fn generate_bindings() {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=src/infrastructure/common/wrapper.hpp");
    let bindings = bindgen::Builder::default()
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
        // Scrollbar
        .whitelist_function(".*CoolSB.*")
        .whitelist_function("GetIconThemePointer")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");
    // Write the bindings to the bindings.rs file.
    let out_path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    bindings
        .write_to_file(
            out_path
                .join("src/infrastructure/common")
                .join("bindings.rs"),
        )
        .expect("Couldn't write bindings!");
}
