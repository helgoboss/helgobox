use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Optionally generate bindings
    #[cfg(feature = "generate")]
    generate_bindings();

    // Compile WDL EEL
    compile_eel();
    Ok(())
}

fn compile_eel() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let asm_object_file = if cfg!(target_os = "windows") {
        if target_arch == "x86_64" {
            Some("../main/lib/WDL/WDL/eel2/asm-nseel-x64.obj")
        } else {
            None
        }
    } else if cfg!(target_os = "macos") {
        if target_arch == "x86_64" {
            Some("../main/lib/WDL/WDL/eel2/asm-nseel-x64-macho.o")
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
            Some("../main/lib/WDL/WDL/eel2/asm-nseel-x64.o")
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
        .file("../main/lib/WDL/WDL/eel2/nseel-cfunc.c")
        .file("../main/lib/WDL/WDL/eel2/nseel-compiler.c")
        .file("../main/lib/WDL/WDL/eel2/nseel-caltab.c")
        .file("../main/lib/WDL/WDL/eel2/nseel-eval.c")
        .file("../main/lib/WDL/WDL/eel2/nseel-lextab.c")
        .file("../main/lib/WDL/WDL/eel2/nseel-ram.c")
        .file("../main/lib/WDL/WDL/eel2/nseel-yylex.c");
    if target_arch == "arm" {
        // To make it compile for Linux armv7 targets.
        build.flag_if_supported("-marm");
    }
    if let Some(f) = asm_object_file {
        build.object(f);
    }
    build.compile("wdl-eel");
}

#[cfg(feature = "generate")]
fn generate_bindings() {
    println!("cargo:rerun-if-changed=src/wrapper.hpp");
    let mut builder = bindgen::Builder::default()
        .header("src/wrapper.hpp")
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
        .write_to_file(out_path.join("src/bindings.rs"))
        .expect("Couldn't write bindings!");
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
