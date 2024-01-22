use crate::bindings::luau::luau_converter::Hook;
use std::fs;
use std::path::PathBuf;
use stylua_lib::OutputVerification;

mod luau_converter;

/// The final code formatting causes error `has overflowed its stack` by default. You need to set
/// `RUST_MIN_STACK` environment variable (e.g. `RUST_MIN_STACK=104857600`) or execute the test in
/// release mode for this to work.
#[test]
pub fn export_luau() {
    struct RealearnApiExportHook;
    impl Hook for RealearnApiExportHook {
        fn translate_crate_name(&self, rust_crate_ident: &str) -> Option<&'static str> {
            match rust_crate_ident {
                "playtime_api" => Some("playtime"),
                _ => None,
            }
        }
    }
    export_luau_internal(
        "realearn",
        [
            "src/persistence/compartment.rs",
            "src/persistence/glue.rs",
            "src/persistence/group.rs",
            "src/persistence/mapping.rs",
            "src/persistence/parameter.rs",
            "src/persistence/source.rs",
            "src/persistence/target.rs",
        ],
        &RealearnApiExportHook,
        r#"require("playtime.luau")"#,
    );
    struct PlaytimeApiExportHook;
    impl Hook for PlaytimeApiExportHook {
        fn include_type(&self, simple_ident: &str) -> bool {
            !matches!(simple_ident, "FlexibleMatrix" | "PlaytimeApiError")
        }
    }
    export_luau_internal(
        "playtime",
        ["../playtime-api/src/persistence/mod.rs"],
        &PlaytimeApiExportHook,
        "",
    );
}

fn export_luau_internal<'a>(
    name: &str,
    src_files: impl IntoIterator<Item = &'a str>,
    hook: &impl Hook,
    leading_lines: &str,
) {
    let rust_codes: Vec<_> = src_files
        .into_iter()
        .map(|src_file| {
            let code = fs::read_to_string(src_file).unwrap();
            let filtered_code: Vec<_> = code
                .lines()
                .filter(|line| !line.starts_with("//!"))
                .collect();
            filtered_code.join("\n")
        })
        .collect();
    let merged_rust_code = rust_codes.join("\n\n");
    let rust_file = syn::parse_file(&merged_rust_code).expect("unable to parse Rust file");
    let luau_file = luau_converter::LuauFile::new(&rust_file, hook);
    let luau_code = format!(
        r#"
        {leading_lines}
        {luau_file}
        "#
    );
    let luau_code = stylua_lib::format_code(
        &luau_code,
        Default::default(),
        None,
        OutputVerification::Full,
    )
    .unwrap();
    let dest_file = PathBuf::from(format!("src/bindings/luau/generated/{name}.luau"));
    fs::write(&dest_file, luau_code).unwrap();
}
