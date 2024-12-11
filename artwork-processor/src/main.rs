use anyhow::{Context, Result};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use resvg::usvg::Options;
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn main() -> Result<()> {
    render_artwork()?;
    println!("Finished rendering artwork");
    Ok(())
}

fn render_artwork() -> Result<()> {
    let playtime_logo_file = "resources/artwork/playtime-logo.svg";
    generate_toolbar_icons("playtime", playtime_logo_file, "")?;
    generate_toolbar_icons("playtime_custom", playtime_logo_file, "with-custom-icon")?;
    generate_icon(
        playtime_logo_file,
        "doc/playtime/modules/ROOT/images/screenshots/playtime-toolbar-icon.png",
        (120, 120),
        "",
        &[ToolbarIconStatus::Normal],
    )?;
    generate_icon(
        playtime_logo_file,
        "doc/playtime/modules/ROOT/images/screenshots/playtime-custom-toolbar-icon.png",
        (120, 120),
        "with-custom-icon",
        &[ToolbarIconStatus::Normal],
    )?;
    Ok(())
}

fn generate_toolbar_icons(name: &str, src_file: &str, additional_root_classes: &str) -> Result<()> {
    use ToolbarIconStatus::*;
    let toolbar_statuses = [Normal, Hovered, Selected];
    generate_icon(
        src_file,
        format!("resources/artwork/toolbar_icons/toolbar_{name}.png"),
        (30, 30),
        additional_root_classes,
        &toolbar_statuses,
    )?;
    generate_icon(
        src_file,
        format!("resources/artwork/toolbar_icons/150/toolbar_{name}.png"),
        (45, 45),
        additional_root_classes,
        &toolbar_statuses,
    )?;
    generate_icon(
        src_file,
        format!("resources/artwork/toolbar_icons/200/toolbar_{name}.png"),
        (60, 60),
        additional_root_classes,
        &toolbar_statuses,
    )?;
    Ok(())
}

fn generate_icon(
    src_file: impl AsRef<Path>,
    dst_file: impl AsRef<Path>,
    (width, height): (u32, u32),
    additional_root_classes: &str,
    statuses: &[ToolbarIconStatus],
) -> Result<()> {
    let dst_file = dst_file.as_ref();
    let svg = fs::read_to_string(src_file)?;
    let pixmap = render_toolbar_icon(&svg, (width, height), additional_root_classes, statuses)?;
    fs::create_dir_all(dst_file.parent().context("no parent file")?)?;
    pixmap.save_png(dst_file)?;
    Ok(())
}

fn render_toolbar_icon(
    svg: &str,
    (width, height): (u32, u32),
    additional_root_classes: &str,
    statuses: &[ToolbarIconStatus],
) -> Result<Pixmap> {
    let sprite_count = statuses.len() as u32;
    let mut pixmap = Pixmap::new(width * sprite_count, height).unwrap();
    use ToolbarIconStatus::*;
    for (i, status) in statuses.iter().enumerate() {
        let root_classes = match status {
            Normal => "toolbar-icon",
            Hovered => "toolbar-icon hovered",
            Selected => "toolbar-icon selected",
        };
        let interpolated_svg = svg
            .replace(
                "ROOT_CLASSES_PLACEHOLDER",
                &format!("{root_classes} {additional_root_classes}"),
            )
            .replace("var(--toolbar-icon-color)", "#818989")
            .replace("var(--toolbar-icon-background-color)", "#f5f5f5")
            .replace("var(--toolbar-icon-hovered-color)", "#939a9a")
            .replace("var(--toolbar-icon-selected-color)", "#1abc98");
        let mut options = Options::default();
        let mut font_db = usvg::fontdb::Database::new();
        font_db.load_fonts_dir("resources/artwork/fonts");
        options.fontdb = Arc::new(font_db);
        let tree = usvg::Tree::from_str(&interpolated_svg, &options)?;
        // Render sprite
        let transform = Transform::from_scale(
            width as f32 / tree.size().width(),
            height as f32 / tree.size().height(),
        )
        .post_translate(i as f32 * width as f32, 0.0);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
    }
    Ok(pixmap)
}

enum ToolbarIconStatus {
    Normal,
    Hovered,
    Selected,
}
