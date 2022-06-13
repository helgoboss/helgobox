use crate::base::*;
use crate::constants::{FOOTER_PANEL_HEIGHT, MAIN_PANEL_WIDTH};
use crate::ext::divider;

pub fn create(
    mut context: ScopedContext,
    effective_header_panel_height: u32,
    effective_rows_panel_height: u32,
) -> Dialog {
    use Style::*;
    let footer_y_offset = effective_header_panel_height + effective_rows_panel_height;
    let controls = vec![
        ctext(
            "ReaLearn",
            context.named_id("ID_MAIN_PANEL_VERSION_TEXT"),
            context.rect(66, footer_y_offset + 19, 337, 9),
        ),
        ctext(
            "Status",
            context.named_id("ID_MAIN_PANEL_STATUS_TEXT"),
            context.rect(56, footer_y_offset + 4, 356, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            " Edit tags...",
            context.named_id("IDC_EDIT_TAGS_BUTTON"),
            context.rect(416, footer_y_offset + 10, 46, 14),
        ),
        divider(
            context.id(),
            context.rect(0, footer_y_offset, MAIN_PANEL_WIDTH, 1),
        ),
    ];
    Dialog {
        id: context.named_id("ID_MAIN_PANEL"),
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
