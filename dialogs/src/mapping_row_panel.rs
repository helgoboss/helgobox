use crate::base::*;
use crate::ext::*;

pub fn create(context: &mut Context) -> Dialog {
    use Style::*;
    let controls = [
        // Label and on/off checkbox
        ltext(
            "Mapping 1",
            context.named_id("ID_MAPPING_ROW_MAPPING_LABEL"),
            Rect::new(14, 3, 225, 9),
            Styles(vec![NOT_WS_GROUP]),
        ),
        checkbox(
            "",
            context.named_id("IDC_MAPPING_ROW_ENABLED_CHECK_BOX"),
            Rect::new(2, 2, 10, 10),
            Styles(vec![WS_GROUP]),
        ),
        // Mapping actions
        pushbutton(
            "Edit",
            context.named_id("ID_MAPPING_ROW_EDIT_BUTTON"),
            Rect::new(347, 13, 31, 14),
            Styles(vec![NOT_WS_TABSTOP]),
        ),
        pushbutton(
            "Duplicate",
            context.named_id("ID_MAPPING_ROW_DUPLICATE_BUTTON"),
            Rect::new(378, 13, 37, 14),
            Styles(vec![NOT_WS_TABSTOP]),
        ),
        pushbutton(
            "Remove",
            context.named_id("ID_MAPPING_ROW_REMOVE_BUTTON"),
            Rect::new(416, 13, 31, 14),
            Styles(vec![NOT_WS_TABSTOP]),
        ),
        pushbutton(
            "Learn source",
            context.named_id("ID_MAPPING_ROW_LEARN_SOURCE_BUTTON"),
            Rect::new(347, 28, 47, 14),
            Styles(vec![WS_GROUP, NOT_WS_TABSTOP]),
        ),
        pushbutton(
            "Learn target",
            context.named_id("ID_MAPPING_ROW_LEARN_TARGET_BUTTON"),
            Rect::new(394, 28, 53, 14),
            Styles(vec![NOT_WS_TABSTOP]),
        ),
        // Control/feedback checkboxes
        simple_checkbox(
            "=>",
            context.named_id("ID_MAPPING_ROW_CONTROL_CHECK_BOX"),
            Rect::new(138, 15, 24, 8),
        ),
        simple_checkbox(
            "<=",
            context.named_id("ID_MAPPING_ROW_FEEDBACK_CHECK_BOX"),
            Rect::new(138, 30, 24, 8),
        ),
        // Source and target labels
        ctext(
            "MIDI CC Value (ch1, cc5)\r\nbla\r\nbla",
            context.named_id("ID_MAPPING_ROW_SOURCE_LABEL_TEXT"),
            Rect::new(43, 12, 94, 34),
            Styles(vec![NOT_WS_GROUP]),
        ),
        ctext(
            "FX Param Target\r\nbla\r\nbla\r\nmoin",
            context.named_id("ID_MAPPING_ROW_TARGET_LABEL_TEXT"),
            Rect::new(161, 12, 182, 34),
            Styles(vec![NOT_WS_GROUP]),
        ),
        // Divider
        divider(
            context.named_id("ID_MAPPING_ROW_DIVIDER"),
            Rect::new(0, 46, 470, 1),
        ),
        // Group label
        rtext(
            "Group 1",
            context.named_id("ID_MAPPING_ROW_GROUP_LABEL"),
            Rect::new(245, 3, 202, 9),
            Styles(vec![NOT_WS_GROUP]),
        ),
        // Match indicator
        ltext(
            "•",
            context.named_id("IDC_MAPPING_ROW_MATCHED_INDICATOR_TEXT"),
            Rect::new(3, 23, 8, 8),
            Styles(vec![WS_DISABLED]),
        ),
        // Up/down buttons
        groupbox(
            "Up",
            context.id(),
            Rect::new(13, 13, 26, 14),
            Styles(vec![WS_GROUP]),
        ),
        pushbutton(
            "Up",
            context.named_id("ID_UP_BUTTON"),
            Rect::new(13, 13, 26, 14),
            Styles(vec![]),
        ),
        pushbutton(
            "Down",
            context.named_id("ID_DOWN_BUTTON"),
            Rect::new(13, 28, 26, 14),
            Styles(vec![]),
        ),
    ];
    Dialog {
        id: context.named_id("ID_MAPPING_ROW_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: Rect::new(0, 0, 460, 48),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD]),
        controls: controls.into_iter().collect(),
        ..context.default_dialog()
    }
}
