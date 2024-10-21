use crate::base::*;
use crate::ext::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    let col_1_x = 0;
    let line_1_y = 0;
    let line_2_y = line_1_y + 20;
    let controls = vec![
        // Name
        ltext(
            "Name",
            ids.named_id("ID_MAPPING_NAME_LABEL"),
            context.rect(col_1_x, line_1_y + 3, 20, 9),
        ) + NOT_WS_GROUP,
        edittext(
            ids.named_id("ID_MAPPING_NAME_EDIT_CONTROL"),
            context.rect(col_1_x + 28, line_1_y, 131, 14),
        ) + ES_AUTOHSCROLL,
        // Tags
        ltext(
            "Tags",
            ids.named_id("ID_MAPPING_TAGS_LABEL"),
            context.rect(col_1_x + 167, line_1_y + 3, 18, 9),
        ) + NOT_WS_GROUP,
        edittext(
            ids.named_id("ID_MAPPING_TAGS_EDIT_CONTROL"),
            context.rect(col_1_x + 189, line_1_y, 131, 14),
        ) + ES_AUTOHSCROLL,
        // Control/feedback checkboxes
        context.checkbox(
            "=> Control",
            ids.named_id("ID_MAPPING_CONTROL_ENABLED_CHECK_BOX"),
            rect(col_1_x + 325, line_1_y + 3, 50, 8),
        ) + WS_TABSTOP,
        context.checkbox(
            "<= Feedback",
            ids.named_id("ID_MAPPING_FEEDBACK_ENABLED_CHECK_BOX"),
            rect(col_1_x + 376, line_1_y + 3, 56, 8),
        ) + WS_TABSTOP,
        // Conditional activation
        ltext(
            "Active",
            ids.named_id("ID_MAPPING_ACTIVATION_TYPE_LABEL"),
            context.rect(col_1_x, line_2_y + 2, 21, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_MAPPING_ACTIVATION_TYPE_COMBO_BOX"),
            context.rect(col_1_x + 28, line_2_y, 102, 15),
        ) + WS_TABSTOP,
        // Conditional activation criteria 1
        ltext(
            "Modifier 1",
            ids.named_id("ID_MAPPING_ACTIVATION_SETTING_1_LABEL_TEXT"),
            context.rect(col_1_x + 138, line_2_y + 2, 34, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Pick 1",
            ids.named_id("ID_MAPPING_ACTIVATION_SETTING_1_BUTTON"),
            context.rect(col_1_x + 177, line_2_y, 90, 15),
        ) + WS_TABSTOP,
        context.checkbox(
            "",
            ids.named_id("ID_MAPPING_ACTIVATION_SETTING_1_CHECK_BOX"),
            rect(col_1_x + 269, line_2_y + 2, 11, 8),
        ) + WS_TABSTOP,
        // Conditional activation criteria 2
        ltext(
            "Modifier 2",
            ids.named_id("ID_MAPPING_ACTIVATION_SETTING_2_LABEL_TEXT"),
            context.rect(col_1_x + 287, line_2_y + 2, 34, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Pick 1",
            ids.named_id("ID_MAPPING_ACTIVATION_SETTING_2_BUTTON"),
            context.rect(col_1_x + 325, line_2_y, 90, 15),
        ) + WS_TABSTOP,
        context.checkbox(
            "",
            ids.named_id("ID_MAPPING_ACTIVATION_SETTING_2_CHECK_BOX"),
            rect(col_1_x + 417, line_2_y + 2, 11, 8),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_MAPPING_ACTIVATION_EDIT_CONTROL"),
            context.rect(col_1_x + 325, line_2_y, 90, 14),
        ) + ES_AUTOHSCROLL,
    ];
    Dialog {
        id: ids.named_id("ID_SHARED_GROUP_MAPPING_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: context.rect(0, 0, 440, 37),
        styles: Styles(vec![
            DS_SETFONT, DS_CONTROL, DS_CENTER, WS_CHILD, WS_VISIBLE, WS_SYSMENU,
        ]),
        controls,
        ..context.default_dialog()
    }
}
