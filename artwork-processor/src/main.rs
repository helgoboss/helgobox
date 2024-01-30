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
    generate_toolbar_icons("playtime-logo.svg")?;
    Ok(())
}

fn generate_toolbar_icons(src_file: &str) -> Result<()> {
    generate_toolbar_icon(src_file, "toolbar_icons/toolbar_playtime.png", (30, 30))?;
    generate_toolbar_icon(src_file, "toolbar_icons/150/toolbar_playtime.png", (45, 45))?;
    generate_toolbar_icon(src_file, "toolbar_icons/200/toolbar_playtime.png", (60, 60))?;
    Ok(())
}

fn generate_toolbar_icon(
    src_file: impl AsRef<Path>,
    dst_file: impl AsRef<Path>,
    (width, height): (u32, u32),
) -> Result<()> {
    let artwork_dir = Path::new("resources/artwork");
    let svg = fs::read_to_string(artwork_dir.join(src_file))?;
    let pixmap = render_toolbar_icon(&svg, (width, height))?;
    let abs_dst_file = artwork_dir.join(dst_file);
    fs::create_dir_all(abs_dst_file.parent().context("no parent file")?)?;
    pixmap.save_png(abs_dst_file)?;
    Ok(())
}

fn render_toolbar_icon(svg: &str, (width, height): (u32, u32)) -> Result<Pixmap> {
    let sprite_count = 3;
    let mut pixmap = Pixmap::new(width * sprite_count, height).unwrap();
    use ToolbarIconStatus::*;
    for (i, status) in [Normal, Hovered, Selected].iter().enumerate() {
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
