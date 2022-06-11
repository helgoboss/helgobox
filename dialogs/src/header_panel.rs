use crate::base::*;
use crate::ext::*;

pub fn create(mut context: ScopedContext) -> Dialog {
    use Style::*;
    let text_height = 9;
    let left_label_x = 7;
    let io_combo_box_x = 68;
    let io_combo_box_dim = Dimensions(194, 16);
    let upper_part_controls = [
        // Input/output
        ltext(
            "Control input",
            context.id(),
            context.rect(left_label_x, 6, 42, text_height),
        ),
        dropdown(
            context.named_id("ID_CONTROL_DEVICE_COMBO_BOX"),
            context.rect_flexible(Point(io_combo_box_x, 4).with_dimensions(io_combo_box_dim)),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        ltext(
            "Feedback output",
            context.id(),
            context.rect(left_label_x, 26, 57, text_height),
        ),
        dropdown(
            context.named_id("ID_FEEDBACK_DEVICE_COMBO_BOX"),
            context.rect_flexible(Point(io_combo_box_x, 24).with_dimensions(io_combo_box_dim)),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        // Quick actions
        pushbutton(
            "Import from clipboard",
            context.named_id("ID_IMPORT_BUTTON"),
            context.rect(270, 3, 73, 14),
        ) + WS_GROUP,
        pushbutton(
            "Export to clipboard",
            context.named_id("ID_EXPORT_BUTTON"),
            context.rect(346, 3, 67, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Projection",
            context.named_id("ID_PROJECTION_BUTTON"),
            context.rect(416, 3, 47, 14),
        ) + NOT_WS_TABSTOP,
        // Event filter
        ltext(
            "Let through:",
            context.named_id("ID_LET_THROUGH_LABEL_TEXT"),
            context.rect(270, 26, 39, 9),
        ),
        checkbox(
            "Matched events",
            context.named_id("ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX"),
            context.rect(319, 26, 67, 8),
        ) + WS_TABSTOP,
        checkbox(
            "Unmatched events",
            context.named_id("ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX"),
            context.rect(392, 26, 76, 8),
        ) + WS_TABSTOP,
    ];
    let show_controls = [
        ltext("Show", context.id(), context.rect(7, 48, 24, 9)),
        radio_button(
            "Controller compartment (for picking a controller preset)",
            context.named_id("ID_CONTROLLER_COMPARTMENT_RADIO_BUTTON"),
            context.rect(60, 48, 185, 8),
        ) + WS_TABSTOP,
        radio_button(
            "Main compartment (for the real mappings)",
            context.named_id("ID_MAIN_COMPARTMENT_RADIO_BUTTON"),
            context.rect(289, 48, 145, 8),
        ) + WS_TABSTOP,
    ];
    let lower_part_controls = [
        // Preset
        ltext(
            "Controller preset",
            context.named_id("ID_PRESET_LABEL_TEXT"),
            context.rect(7, 69, 57, 9),
        ),
        dropdown(
            context.named_id("ID_PRESET_COMBO_BOX"),
            context.rect(68, 67, 135, 16),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        // Preset actions
        pushbutton(
            "Save as...",
            context.named_id("ID_PRESET_SAVE_AS_BUTTON"),
            context.rect(234, 66, 42, 14),
        ) + WS_GROUP,
        pushbutton(
            "Save",
            context.named_id("ID_PRESET_SAVE_BUTTON"),
            context.rect(207, 66, 26, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Delete",
            context.named_id("ID_PRESET_DELETE_BUTTON"),
            context.rect(278, 66, 28, 14),
        ) + NOT_WS_TABSTOP,
        // Auto-load
        ltext(
            "Auto-load",
            context.named_id("ID_AUTO_LOAD_LABEL_TEXT"),
            context.rect(319, 69, 33, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_AUTO_LOAD_COMBO_BOX"),
            context.rect(356, 67, 107, 16),
        ) + WS_VSCROLL
            + WS_GROUP
            + WS_TABSTOP,
        // Mapping group
        ltext("Mapping group", context.id(), context.rect(7, 89, 55, 9)) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_GROUP_COMBO_BOX"),
            context.rect(68, 87, 135, 16),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        // Mapping group actions
        pushbutton(
            "Add",
            context.named_id("ID_GROUP_ADD_BUTTON"),
            context.rect(207, 86, 26, 14),
        ) + WS_GROUP,
        pushbutton(
            "Remove",
            context.named_id("ID_GROUP_DELETE_BUTTON"),
            context.rect(234, 86, 42, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Edit",
            context.named_id("ID_GROUP_EDIT_BUTTON"),
            context.rect(278, 86, 28, 14),
        ) + NOT_WS_TABSTOP,
        // Mapping list actions
        ltext("Mappings", context.id(), context.rect(7, 109, 33, 9)) + NOT_WS_GROUP,
        pushbutton(
            "Add one",
            context.named_id("ID_ADD_MAPPING_BUTTON"),
            context.rect(42, 106, 41, 14),
        ) + WS_GROUP,
        pushbutton(
            "Learn many",
            context.named_id("ID_LEARN_MANY_MAPPINGS_BUTTON"),
            context.rect(86, 106, 47, 14),
        ) + NOT_WS_TABSTOP,
        // Search
        ltext("Search", context.id(), context.rect(139, 109, 25, 9)) + NOT_WS_GROUP,
        edittext(
            context.named_id("ID_HEADER_SEARCH_EDIT_CONTROL"),
            context.rect(165, 106, 157, 14),
        ) + ES_AUTOHSCROLL,
        pushbutton(
            "X",
            context.named_id("ID_CLEAR_SEARCH_BUTTON"),
            context.rect(323, 106, 11, 14),
        ) + NOT_WS_TABSTOP,
        // Source filter
        pushbutton(
            "Filter source",
            context.named_id("ID_FILTER_BY_SOURCE_BUTTON"),
            context.rect(340, 106, 48, 14),
        ) + WS_GROUP,
        pushbutton(
            "X",
            context.named_id("ID_CLEAR_SOURCE_FILTER_BUTTON"),
            context.rect(388, 106, 11, 14),
        ) + NOT_WS_TABSTOP,
        // Target filter
        pushbutton(
            "Filter target",
            context.named_id("ID_FILTER_BY_TARGET_BUTTON"),
            context.rect(406, 106, 45, 14),
        ) + WS_GROUP,
        pushbutton(
            "X",
            context.named_id("ID_CLEAR_TARGET_FILTER_BUTTON"),
            context.rect(452, 106, 11, 14),
        ) + NOT_WS_TABSTOP,
    ];
    let divider_controls = [
        divider(context.id(), context.rect(0, 41, 470, 1)),
        divider(context.id(), context.rect(0, 123, 470, 1)),
        divider(context.id(), context.rect(0, 62, 470, 1)),
    ];
    Dialog {
        id: context.named_id("ID_HEADER_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: context.rect(0, 0, 470, 124),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD, WS_VISIBLE]),
        controls: upper_part_controls
            .into_iter()
            .chain(show_controls.into_iter())
            .chain(lower_part_controls.into_iter())
            .chain(divider_controls.into_iter())
            .collect(),
        ..context.default_dialog()
    }
}
