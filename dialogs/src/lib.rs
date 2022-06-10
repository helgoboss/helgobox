use crate::base::{Context, Dialog, Font, Resource};
use std::path::Path;

mod base;
mod ext;
mod group_panel;
mod header_panel;
mod mapping_row_panel;

pub fn generate_dialog_files(out_dir: impl AsRef<Path>) {
    let default_font = Font {
        name: "Ms Shell Dlg",
        size: 8,
    };
    let default_dialog = Dialog {
        font: Some(default_font),
        ..Default::default()
    };
    let mut context = Context::new(30000, default_dialog);
    let resource = Resource {
        dialogs: vec![
            group_panel::create(&mut context),
            header_panel::create(&mut context),
            mapping_row_panel::create(&mut context),
        ],
    };
    // Write header file
    let header_file_content = resource.generate_header().to_string();
    std::fs::write(out_dir.as_ref().join("resource2.h"), header_file_content)
        .expect("couldn't write header file");
    // Write rc file
    let rc_file_header = include_str!("rc_file_header.txt");
    let rc_file_footer = include_str!("rc_file_footer.txt");
    let rc_file_content = format!("{}\n\n{}\n\n{}", rc_file_header, resource, rc_file_footer);
    std::fs::write(out_dir.as_ref().join("msvc2.rc"), rc_file_content)
        .expect("couldn't write rc file");
}
