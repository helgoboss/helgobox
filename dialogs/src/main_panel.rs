use crate::base::*;
use crate::constants::{FOOTER_PANEL_HEIGHT, MAIN_PANEL_WIDTH};
use crate::ext::divider;

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
    let text_line_left = 66;
    let text_line_right = MAIN_PANEL_WIDTH - text_line_left;
    let text_line_width = text_line_right - text_line_left;
    let controls = vec![
        divider(ids.id(), create_rect(0, 0, MAIN_PANEL_WIDTH, 1)),
        ctext(
            "Status 1",
            ids.named_id("ID_MAIN_PANEL_STATUS_1_TEXT"),
            create_rect(text_line_left, 5, text_line_width, 9),
        ) + NOT_WS_GROUP,
        ctext(
            "Status 2",
            ids.named_id("ID_MAIN_PANEL_STATUS_2_TEXT"),
            create_rect(text_line_left, 5 + line_spacing, text_line_width, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Unit",
            ids.named_id("IDC_UNIT_BUTTON"),
            create_rect(8, 5 + line_spacing, 56, 14),
        ),
        pushbutton(
            "Unit data...",
            ids.named_id("IDC_EDIT_TAGS_BUTTON"),
            create_rect(406, 5 + line_spacing, 56, 14),
        ),
        ctext(
            "ReaLearn",
            ids.named_id("ID_MAIN_PANEL_VERSION_TEXT"),
            create_rect(text_line_left, 5 + line_spacing * 2, text_line_width, 9),
        ),
    ];
    Dialog {
        id: ids.named_id("ID_MAIN_PANEL"),
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
