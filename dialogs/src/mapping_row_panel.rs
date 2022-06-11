use crate::base::*;
use crate::ext::*;

pub fn create(mut context: ScopedContext) -> Dialog {
    use Style::*;
    let controls = [
        // Label and on/off checkbox
        ltext(
            "Mapping 1",
            context.named_id("ID_MAPPING_ROW_MAPPING_LABEL"),
            context.rect(14, 3, 225, 9),
        ) + NOT_WS_GROUP,
        checkbox(
            "",
            context.named_id("IDC_MAPPING_ROW_ENABLED_CHECK_BOX"),
            context.rect(2, 2, 10, 10),
        ) + WS_GROUP,
        // Mapping actions
        pushbutton(
            "Edit",
            context.named_id("ID_MAPPING_ROW_EDIT_BUTTON"),
            context.rect(347, 13, 31, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Duplicate",
            context.named_id("ID_MAPPING_ROW_DUPLICATE_BUTTON"),
            context.rect(378, 13, 37, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Remove",
            context.named_id("ID_MAPPING_ROW_REMOVE_BUTTON"),
            context.rect(416, 13, 31, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Learn source",
            context.named_id("ID_MAPPING_ROW_LEARN_SOURCE_BUTTON"),
            context.rect(347, 28, 47, 14),
        ) + WS_GROUP
            + NOT_WS_TABSTOP,
        pushbutton(
            "Learn target",
            context.named_id("ID_MAPPING_ROW_LEARN_TARGET_BUTTON"),
            context.rect(394, 28, 53, 14),
        ) + NOT_WS_TABSTOP,
        // Control/feedback checkboxes
        checkbox(
            "=>",
            context.named_id("ID_MAPPING_ROW_CONTROL_CHECK_BOX"),
            context.rect(138, 15, 24, 8),
        ),
        checkbox(
            "<=",
            context.named_id("ID_MAPPING_ROW_FEEDBACK_CHECK_BOX"),
            context.rect(138, 30, 24, 8),
        ),
        // Source and target labels
        ctext(
            "MIDI CC Value (ch1, cc5)\r\nbla\r\nbla",
            context.named_id("ID_MAPPING_ROW_SOURCE_LABEL_TEXT"),
            context.rect(43, 12, 94, 34),
        ) + NOT_WS_GROUP,
        ctext(
            "FX Param Target\r\nbla\r\nbla\r\nmoin",
            context.named_id("ID_MAPPING_ROW_TARGET_LABEL_TEXT"),
            context.rect(161, 12, 182, 34),
        ) + NOT_WS_GROUP,
        // Divider
        divider(
            context.named_id("ID_MAPPING_ROW_DIVIDER"),
            context.rect(0, 46, 470, 1),
        ),
        // Group label
        rtext(
            "Group 1",
            context.named_id("ID_MAPPING_ROW_GROUP_LABEL"),
            context.rect(245, 3, 202, 9),
        ) + NOT_WS_GROUP,
        // Match indicator
        ltext(
            "•",
            context.named_id("IDC_MAPPING_ROW_MATCHED_INDICATOR_TEXT"),
            context.rect(3, 23, 8, 8),
        ) + WS_DISABLED,
        // Up/down buttons
        groupbox("Up", context.id(), context.rect(13, 13, 26, 14)) + WS_GROUP,
        pushbutton(
            "Up",
            context.named_id("ID_UP_BUTTON"),
            context.rect(13, 13, 26, 14),
        ),
        pushbutton(
            "Down",
            context.named_id("ID_DOWN_BUTTON"),
            context.rect(13, 28, 26, 14),
        ),
    ];
    Dialog {
        id: context.named_id("ID_MAPPING_ROW_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: context.rect(0, 0, 460, 48),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD]),
        controls: controls.into_iter().collect(),
        ..context.default_dialog()
    }
}
