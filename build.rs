fn main() {
    #[cfg(feature = "generate")]
    generate_bindings();
    let target = std::env::var("TARGET").unwrap();
    if let Some(tool) = cc::windows_registry::find_tool(target.as_str(), "cl.exe") {
        for (key, value) in tool.env() {
            std::env::set_var(key, value);
        }
    }
    embed_resource::compile("src/infrastructure/common/realearn.rc");
}

fn generate_bindings() {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=src/infrastructure/ui/wrapper.hpp");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.

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
        // .whitelist_var("CSURF_EXT_.*")
        // .whitelist_type("HINSTANCE")
        .whitelist_function("DefWindowProc")
        .whitelist_function("DefWindowProcA")
        .whitelist_function("CreateDialogParamA")
        .whitelist_function("DestroyWindow")
        .whitelist_function("GetDlgItem")
        .whitelist_function("ShowWindow")
        .whitelist_function("SetDlgItemText")
        .whitelist_function("SetWindowTextA")
        .whitelist_function("MAKEINTRESOURCEA")
        .whitelist_type("ULONG_PTR")
        .whitelist_type("HINSTANCE")
        .whitelist_type("HWND")
        .whitelist_type("LPARAM")
        .whitelist_type("LRESULT")
        .whitelist_type("LPSTR")
        .whitelist_type("BOOL")
        .whitelist_type("WORD")
        .whitelist_type("UINT")
        .whitelist_var("SWELL_curmodule_dialogresource_head")
        .whitelist_var("WM_CLOSE")
        .whitelist_var("WM_COMMAND")
        .whitelist_var("WM_DESTROY")
        .whitelist_var("WM_INITDIALOG")
        .whitelist_var("SW_SHOW")
        .whitelist_function("SWELL_CreateDialog")
        .whitelist_type("WPARAM")
        // ReaLearn UI
        .whitelist_var("ID_.*")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");
    // Write the bindings to the bindings.rs file.
    let out_path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let file_name = if cfg!(target_os = "linux") {
        "bindings_linux.rs"
    } else {
        "bindings_windows.rs"
    };
    bindings
        .write_to_file(out_path.join("src/infrastructure/common").join(file_name))
        .expect("Couldn't write bindings!");
}
