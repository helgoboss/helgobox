use crate::base::*;
use crate::constants::{FOOTER_PANEL_HEIGHT, MAIN_PANEL_WIDTH};

pub fn create(
    context: ScopedContext,
    ids: &mut IdGenerator,
    effective_header_panel_height: u32,
    effective_rows_panel_height: u32,
) -> Dialog {
    use Style::*;
    let footer_y_offset = effective_header_panel_height + effective_rows_panel_height;
    let create_rect = |x, y, width, height| {
        let local_rect = context.rect(x, y, width, height);
        Rect {
            y: footer_y_offset + local_rect.y,
            ..local_rect
        }
    };
    let line_spacing = 12;
    let controls = vec![pushbutton(
        "Unit",
        ids.named_id("IDC_UNIT_BUTTON"),
        create_rect(8, 5 + line_spacing, 56, 14),
    )];
    Dialog {
        id: ids.named_id("ID_INSTANCE_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: Rect::new(
            0,
            0,
            context.scale_width(MAIN_PANEL_WIDTH),
            effective_header_panel_height
                + effective_rows_panel_height
                + context.scale_height(FOOTER_PANEL_HEIGHT),
        ),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD, WS_VISIBLE]),
        controls,
        ..context.default_dialog()
    }
}
