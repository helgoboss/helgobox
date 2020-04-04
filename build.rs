fn main() {
    generate_bindings();
    let target = std::env::var("TARGET").unwrap();
    if let Some(tool) = cc::windows_registry::find_tool(target.as_str(), "cl.exe") {
        for (key, value) in tool.env() {
            std::env::set_var(key, value);
        }
    }
    embed_resource::compile("src/view/realearn.rc");
}

fn generate_bindings() {
    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=src/view/bindgen.hpp");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.

    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("src/view/bindgen.hpp")
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
        // .whitelist_function("GetActiveWindow")
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
        .write_to_file(out_path.join("src/view/bindings.rs"))
        .expect("Couldn't write bindings!");
}
