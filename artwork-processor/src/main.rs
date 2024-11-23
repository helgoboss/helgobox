use anyhow::{Context, Result};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, TreeParsing};
use resvg::{usvg, Tree};
use std::fs;
use std::path::Path;

fn main() -> Result<()> {
    render_artwork()?;
    println!("Finished rendering artwork");
    Ok(())
}

fn render_artwork() -> Result<()> {
    let playtime_logo_file = "resources/artwork/playtime-logo.svg";
    generate_toolbar_icons(playtime_logo_file)?;
    generate_icon(
        playtime_logo_file,
        "doc/playtime/modules/ROOT/images/screenshots/playtime-toolbar-icon.png",
        (120, 120),
        &[ToolbarIconStatus::Normal],
    )?;
    Ok(())
}

fn generate_toolbar_icons(src_file: &str) -> Result<()> {
    use ToolbarIconStatus::*;
    let toolbar_statuses = [Normal, Hovered, Selected];
    generate_icon(
        src_file,
        "resources/artwork/toolbar_icons/toolbar_playtime.png",
        (30, 30),
        &toolbar_statuses,
    )?;
    generate_icon(
        src_file,
        "resources/artwork/toolbar_icons/150/toolbar_playtime.png",
        (45, 45),
        &toolbar_statuses,
    )?;
    generate_icon(
        src_file,
        "resources/artwork/toolbar_icons/200/toolbar_playtime.png",
        (60, 60),
        &toolbar_statuses,
    )?;
    Ok(())
}

fn generate_icon(
    src_file: impl AsRef<Path>,
    dst_file: impl AsRef<Path>,
    (width, height): (u32, u32),
    statuses: &[ToolbarIconStatus],
) -> Result<()> {
    let dst_file = dst_file.as_ref();
    let svg = fs::read_to_string(src_file)?;
    let pixmap = render_toolbar_icon(&svg, (width, height), statuses)?;
    fs::create_dir_all(dst_file.parent().context("no parent file")?)?;
    pixmap.save_png(dst_file)?;
    Ok(())
}

fn render_toolbar_icon(
    svg: &str,
    (width, height): (u32, u32),
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
            .replace("ROOT_CLASSES_PLACEHOLDER", root_classes)
            .replace("var(--toolbar-icon-color)", "#818989")
            .replace("var(--toolbar-icon-hovered-color)", "#939a9a")
            .replace("var(--toolbar-icon-selected-color)", "#1abc98");
        let tree = usvg::Tree::from_str(&interpolated_svg, &Options::default())?;
        // Render sprite
        let render_tree = Tree::from_usvg(&tree);
        let transform = Transform::from_scale(
            width as f32 / tree.size.width(),
            height as f32 / tree.size.height(),
        )
        .post_translate(i as f32 * width as f32, 0.0);
        render_tree.render(transform, &mut pixmap.as_mut());
    }
    Ok(pixmap)
}

enum ToolbarIconStatus {
    Normal,
    Hovered,
    Selected,
}
