use anyhow::{Context, Result};
use resvg::tiny_skia::{Pixmap, PremultipliedColorU8, Transform};
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
    let logo_file = "resources/artwork/playtime-logo.svg";
    let logo_svg = fs::read_to_string(logo_file)?;
    generate_icons("playtime", &logo_svg, "")?;
    generate_icons("playtime-custom", &logo_svg, "with-settings-icon")?;
    let logo_svg_with_text_2 = logo_svg.replace("TEXT_PLACEHOLDER", "2");
    generate_icons("playtime-2", &logo_svg_with_text_2, "with-text-badge")?;
    Ok(())
}

fn generate_icons(name_with_dashes: &str, svg: &str, additional_root_classes: &str) -> Result<()> {
    let name_with_underscores = name_with_dashes.replace('-', "_");
    // Toolbar icons
    generate_toolbar_icons(&name_with_underscores, svg, additional_root_classes)?;
    // Icons for docs
    generate_icon(
        svg,
        format!("doc/playtime/modules/ROOT/images/screenshots/{name_with_dashes}-toolbar-icon.png"),
        (120, 120),
        additional_root_classes,
        &[ToolbarIconStatus::Normal],
    )?;
    Ok(())
}

fn generate_toolbar_icons(name: &str, svg: &str, additional_root_classes: &str) -> Result<()> {
    use ToolbarIconStatus::*;
    let toolbar_statuses = [Normal, Hovered, Selected];
    generate_icon(
        svg,
        format!("resources/artwork/toolbar_icons/toolbar_{name}.png"),
        (30, 30),
        additional_root_classes,
        &toolbar_statuses,
    )?;
    generate_icon(
        svg,
        format!("resources/artwork/toolbar_icons/150/toolbar_{name}.png"),
        (45, 45),
        additional_root_classes,
        &toolbar_statuses,
    )?;
    generate_icon(
        svg,
        format!("resources/artwork/toolbar_icons/200/toolbar_{name}.png"),
        (60, 60),
        additional_root_classes,
        &toolbar_statuses,
    )?;
    Ok(())
}

fn generate_icon(
    svg: &str,
    dst_file: impl AsRef<Path>,
    (width, height): (u32, u32),
    additional_root_classes: &str,
    statuses: &[ToolbarIconStatus],
) -> Result<()> {
    let dst_file = dst_file.as_ref();
    let pixmap = render_toolbar_icon(svg, (width, height), additional_root_classes, statuses)?;
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
        let fg_color = match status {
            Normal => "#818989",
            Hovered => "#939a9a",
            Selected => "#1abc98",
        };
        let interpolated_svg = svg
            .replace(
                "ROOT_CLASSES_PLACEHOLDER",
                &format!("toolbar-icon {additional_root_classes}"),
            )
            .replace("var(--fg-color)", fg_color)
            .replace("var(--bg-color", "#333333");
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
    // Replace "shine-through" color with transparency
    let shine_through_color = PremultipliedColorU8::from_rgba(255, 0, 255, 255).unwrap(); // Magenta
    for pixel in pixmap.pixels_mut() {
        if *pixel == shine_through_color {
            *pixel = PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap(); // Transparent
        }
    }
    Ok(pixmap)
}

enum ToolbarIconStatus {
    Normal,
    Hovered,
    Selected,
}
