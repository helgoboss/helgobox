use std::fs;
use std::path::PathBuf;

mod luau_converter;

#[test]
pub fn export_luau() {
    export_luau_internal(
        "helgobox",
        [
            "src/persistence/compartment.rs",
            "src/persistence/glue.rs",
            "src/persistence/group.rs",
            "src/persistence/mapping.rs",
            "src/persistence/parameter.rs",
            "src/persistence/source.rs",
            "src/persistence/target.rs",
            "../playtime-api/src/persistence/mod.rs",
        ],
        |ident| !matches!(ident, "FlexibleMatrix" | "PlaytimeApiError"),
        // |_| true,
    );
}

fn export_luau_internal<'a>(
    name: &str,
    src_files: impl IntoIterator<Item = &'a str>,
    include_ident: fn(&str) -> bool,
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
    let luau_file = luau_converter::LuauFile::new(&rust_file, include_ident);
    let luau_code = format!(
        r#"
        {luau_file}
        "#
    );
    let dest_file = PathBuf::from(format!("src/bindings/luau/generated/{name}.luau"));
    fs::write(&dest_file, luau_code).unwrap();
}
