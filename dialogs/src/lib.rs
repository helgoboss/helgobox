use crate::base::{
    Context, Dialog, DialogScaling, Font, Resource, ResourceInfoAsCHeaderCode,
    ResourceInfoAsRustCode,
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

pub fn generate_dialog_files(out_dir: impl AsRef<Path>) {
    let default_font = Font {
        name: "Ms Shell Dlg",
        size: 8,
    };
    let default_dialog = Dialog {
        font: Some(default_font),
        ..Default::default()
    };
    // let vertical_scale = 0.8;
    let horizontal_scale = 1.0;
    let vertical_scale = 1.0;
    let mut context = Context {
        next_id_value: 30000,
        default_dialog,
        global_scaling: DialogScaling {
            x_scale: horizontal_scale,
            y_scale: vertical_scale,
            width_scale: horizontal_scale,
            height_scale: vertical_scale,
        },
        dialog_specific_scaling: Default::default(),
    };
    let resource = Resource {
        dialogs: vec![
            group_panel::create(&mut context),
            header_panel::create(&mut context),
            mapping_panel::create(&mut context),
            mapping_row_panel::create(&mut context),
            mapping_rows_panel::create(&mut context),
            message_panel::create(&mut context),
            shared_group_mapping_panel::create(&mut context),
            main_panel::create(&mut context),
            yaml_editor_panel::create(&mut context),
        ],
    };
    let header_info = resource.generate_info(&context);
    // Write C header file (in case we want to use a resource editor to preview the dialogs)
    let c_header_code = ResourceInfoAsCHeaderCode(&header_info).to_string();
    std::fs::write(out_dir.as_ref().join("msvc/resource.h"), c_header_code)
        .expect("couldn't write C header file");
    // Write Rust file (so we don't have to do it via bindgen, which is slow)
    let rust_code = ResourceInfoAsRustCode(&header_info).to_string();
    std::fs::write(out_dir.as_ref().join("bindings.rs"), rust_code)
        .expect("couldn't write Rust bindings file");
    // Write rc file
    let rc_file_header = include_str!("rc_file_header.txt");
    let rc_file_footer = include_str!("rc_file_footer.txt");
    let rc_file_content = format!("{}\n\n{}\n\n{}", rc_file_header, resource, rc_file_footer);
    let mut output = Vec::new();
    // Write UTF_16LE BOM
    output.write_all(&[0xFF, 0xFE]).unwrap();
    for utf16 in rc_file_content.encode_utf16() {
        output.write_all(&utf16.to_le_bytes()).unwrap();
    }
    std::fs::write(out_dir.as_ref().join("msvc/msvc.rc"), output).expect("couldn't write rc file");
}
