use crate::base::*;
use crate::ext::*;

pub fn create(context: &mut Context) -> Dialog {
    use Style::*;
    let text_height = 9;
    let left_label_x = 7;
    let io_combo_box_x = 68;
    let io_combo_box_dim = Dimensions(194, 16);
    let upper_part_controls = [
        ltext(
            "Control input",
            context.id(),
            Rect::new(left_label_x, 6, 42, text_height),
        ),
        dropdown(
            context.named_id("ID_CONTROL_DEVICE_COMBO_BOX"),
            Point(io_combo_box_x, 4).with_dimensions(io_combo_box_dim),
        ),
        ltext(
            "Feedback output",
            context.id(),
            Rect::new(left_label_x, 26, 57, text_height),
        ),
        dropdown(
            context.named_id("ID_FEEDBACK_DEVICE_COMBO_BOX"),
            Point(io_combo_box_x, 24).with_dimensions(io_combo_box_dim),
        ),
        pushbutton(
            "Import from clipboard",
            context.named_id("ID_IMPORT_BUTTON"),
            Rect::new(270, 3, 73, 14),
            Styles(vec![WS_GROUP]),
        ),
        pushbutton(
            "Export to clipboard",
            context.named_id("ID_EXPORT_BUTTON"),
            Rect::new(346, 3, 67, 14),
            Styles(vec![NOT_WS_TABSTOP]),
        ),
        pushbutton(
            "Projection",
            context.named_id("ID_PROJECTION_BUTTON"),
            Rect::new(416, 3, 47, 14),
            Styles(vec![NOT_WS_TABSTOP]),
        ),
        ltext(
            "Let through",
            context.named_id("ID_LET_THROUGH_LABEL_TEXT"),
            Rect::new(270, 26, 39, 9),
        ),
    ];
    Dialog {
        id: context.named_id("ID_HEADER_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: Rect::new(0, 0, 470, 124),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD, WS_VISIBLE]),
        controls: upper_part_controls.into_iter().collect(),
        ..context.default_dialog()
    }
}
