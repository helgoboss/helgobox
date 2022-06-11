use crate::base::{
    Context, Dialog, DialogScaling, Font, Resource, ResourceInfoAsCHeaderCode,
    ResourceInfoAsRustCode, Scope,
};
use std::io::Write;
use std::path::Path;

mod base;
mod ext;
mod group_panel;
mod header_panel;
mod main_panel;
mod mapping_panel;
mod mapping_row_panel;
mod mapping_rows_panel;
mod message_panel;
mod shared_group_mapping_panel;
mod yaml_editor_panel;

pub fn generate_dialog_files(rc_dir: impl AsRef<Path>, bindings_file: impl AsRef<Path>) {
    let default_font = Font {
        name: "Ms Shell Dlg",
        size: 8,
    };
    let default_dialog = Dialog {
        font: Some(default_font),
        ..Default::default()
    };
    // let vertical_scale = 0.8;
    let global_scope = {
        let horizontal_scale = 1.0;
        let vertical_scale = 1.0;
        Scope {
            scaling: DialogScaling {
                x_scale: horizontal_scale,
                y_scale: vertical_scale,
                width_scale: horizontal_scale,
                height_scale: vertical_scale,
            },
        }
    };
    let mapping_panel_scope = {
        let horizontal_scale = 1.0;
        let vertical_scale = 0.8;
        Scope {
            scaling: DialogScaling {
                x_scale: horizontal_scale,
                y_scale: vertical_scale,
                width_scale: horizontal_scale,
                height_scale: vertical_scale,
            },
        }
    };
    let mut context = Context {
        next_id_value: 30000,
        default_dialog,
        scopes: [("MAPPING_PANEL", mapping_panel_scope)]
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
        global_scope,
    };
    let resource = Resource {
        dialogs: vec![
            group_panel::create(context.scoped("MAPPING_PANEL")),
            header_panel::create(context.global()),
            mapping_panel::create(context.scoped("MAPPING_PANEL")),
            mapping_row_panel::create(context.global()),
            mapping_rows_panel::create(context.global()),
            message_panel::create(context.global()),
            shared_group_mapping_panel::create(context.scoped("MAPPING_PANEL")),
            main_panel::create(context.global()),
            yaml_editor_panel::create(context.global()),
        ],
    };
    let header_info = resource.generate_info(&context);
    // Write C header file (in case we want to use a resource editor to preview the dialogs)
    let c_header_code = ResourceInfoAsCHeaderCode(&header_info).to_string();
    std::fs::write(rc_dir.as_ref().join("resource.h"), c_header_code)
        .expect("couldn't write C header file");
    // Write Rust file (so we don't have to do it via bindgen, which is slow)
    let rust_code = ResourceInfoAsRustCode(&header_info).to_string();
    std::fs::write(bindings_file, rust_code).expect("couldn't write Rust bindings file");
    // Write rc file
    let rc_file_header = include_str!("rc_file_header.txt");
    let rc_file_footer = include_str!("rc_file_footer.txt");
    let rc_file_content = format!("{}\n\n{}\n\n{}", rc_file_header, resource, rc_file_footer);
    let mut output = Vec::new();
    // Write UTF_16LE BOM
    output.write_all(&[0xFF, 0xFE]).unwrap();
    // Write UTF_16LE contents
    for utf16 in rc_file_content.encode_utf16() {
        output.write_all(&utf16.to_le_bytes()).unwrap();
    }
    std::fs::write(rc_dir.as_ref().join("msvc.rc"), output).expect("couldn't write rc file");
}
