use crate::base::*;
use crate::constants::{HEADER_PANEL_HEIGHT, HEADER_PANEL_WIDTH};
use crate::ext::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    let text_height = 9;
    let left_label_x = 7;
    let io_label_width = 24;
    let space = 3;
    let (io_combo_box_width, io_combo_box_height) = (190, 14);
    let mut row1 = Adder(left_label_x);
    let mut row2 = Adder(left_label_x);
    let upper_part_controls = [
        // Input/output
        ltext(
            "Input",
            ids.id(),
            context.rect(row1.get(), 5, row1.span(io_label_width), text_height),
        ),
        pushbutton(
            "MIDI: <FX input>",
            ids.named_id("ID_CONTROL_INPUT_BUTTON"),
            context.rect(
                row1.space(space),
                3,
                row1.span(io_combo_box_width),
                io_combo_box_height,
            ),
        ) + NOT_WS_TABSTOP,
        ltext(
            "Output",
            ids.id(),
            context.rect(row2.get(), 25, row2.span(io_label_width), text_height),
        ),
        pushbutton(
            "<None>",
            ids.named_id("ID_FEEDBACK_OUTPUT_BUTTON"),
            context.rect(
                row2.space(space),
                23,
                row2.span(io_combo_box_width),
                io_combo_box_height,
            ),
        ) + NOT_WS_TABSTOP,
        // Quick actions
        pushbutton(
            "Menu",
            ids.named_id("ID_MENU_BUTTON"),
            context.rect(row1.space(space), 3, row1.span(27), 14),
        ) + WS_GROUP,
        pushbutton(
            "Import from clipboard",
            ids.named_id("ID_IMPORT_BUTTON"),
            context.rect(row1.space(space), 3, row1.span(73), 14),
        ) + WS_GROUP,
        pushbutton(
            "Export to clipboard",
            ids.named_id("ID_EXPORT_BUTTON"),
            context.rect(row1.space(space), 3, row1.span(67), 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Projection",
            ids.named_id("ID_PROJECTION_BUTTON"),
            context.rect(row1.space(space), 3, row1.span(42), 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "?",
            ids.named_id("ID_MAIN_HELP_BUTTON"),
            context.rect(row1.space(space), 3, row1.span(14), 14),
        ) + NOT_WS_TABSTOP,
        // Event filter
        ltext(
            "Let MIDI through:",
            ids.named_id("ID_LET_THROUGH_LABEL_TEXT"),
            context.rect(260, 25, 42, 9),
        ),
        context.checkbox(
            "Matched events",
            ids.named_id("ID_LET_MATCHED_EVENTS_THROUGH_CHECK_BOX"),
            rect(319, 25, 67, 8),
        ) + WS_TABSTOP,
        context.checkbox(
            "Unmatched events",
            ids.named_id("ID_LET_UNMATCHED_EVENTS_THROUGH_CHECK_BOX"),
            rect(392, 25, 76, 8),
        ) + WS_TABSTOP,
    ];
    let show_controls = [
        ltext(
            "Show",
            ids.named_id("ID_HEADER_PANEL_SHOW_LABEL_TEXT"),
            context.rect(7, 47, 24, 9),
        ),
        radio_button(
            "Controller compartment (for describing your device, optional)",
            ids.named_id("ID_CONTROLLER_COMPARTMENT_RADIO_BUTTON"),
            context.rect(50, 47, 200, 8),
        ) + WS_TABSTOP,
        radio_button(
            "Main compartment (for defining what it should do)",
            ids.named_id("ID_MAIN_COMPARTMENT_RADIO_BUTTON"),
            context.rect(269, 47, 200, 8),
        ) + WS_TABSTOP,
    ];
    let lower_part_controls = [
        // Preset
        ltext(
            "Controller preset",
            ids.named_id("ID_PRESET_LABEL_TEXT"),
            context.rect(7, 69, 57, 9),
        ),
        pushbutton(
            "<None>",
            ids.named_id("ID_PRESET_BROWSE_BUTTON"),
            context.rect(68, 66, 135, 14),
        ) + WS_GROUP,
        // Preset actions
        pushbutton(
            "Save as...",
            ids.named_id("ID_PRESET_SAVE_AS_BUTTON"),
            context.rect(234, 66, 42, 14),
        ) + WS_GROUP,
        pushbutton(
            "Save",
            ids.named_id("ID_PRESET_SAVE_BUTTON"),
            context.rect(207, 66, 26, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Delete",
            ids.named_id("ID_PRESET_DELETE_BUTTON"),
            context.rect(278, 66, 28, 14),
        ) + NOT_WS_TABSTOP,
        // Auto-load
        ltext(
            "Auto-load",
            ids.named_id("ID_AUTO_LOAD_LABEL_TEXT"),
            context.rect(319, 69, 33, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_AUTO_LOAD_COMBO_BOX"),
            context.rect(356, 67, 107, 16),
        ) + WS_VSCROLL
            + WS_GROUP
            + WS_TABSTOP,
        // Mapping group
        ltext("Mapping group", ids.id(), context.rect(7, 89, 55, 9)) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_GROUP_COMBO_BOX"),
            context.rect(68, 87, 135, 16),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        // Mapping group actions
        pushbutton(
            "Add",
            ids.named_id("ID_GROUP_ADD_BUTTON"),
            context.rect(207, 86, 26, 14),
        ) + WS_GROUP,
        pushbutton(
            "Remove",
            ids.named_id("ID_GROUP_DELETE_BUTTON"),
            context.rect(234, 86, 42, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Edit",
            ids.named_id("ID_GROUP_EDIT_BUTTON"),
            context.rect(278, 86, 28, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Notes",
            ids.named_id("ID_NOTES_BUTTON"),
            context.rect(426, 86, 35, 14),
        ) + NOT_WS_TABSTOP,
        // Mapping list actions
        ltext("Mappings", ids.id(), context.rect(7, 109, 33, 9)) + NOT_WS_GROUP,
        pushbutton(
            "Add one",
            ids.named_id("ID_ADD_MAPPING_BUTTON"),
            context.rect(42, 106, 41, 14),
        ) + WS_GROUP,
        pushbutton(
            "Learn many",
            ids.named_id("ID_LEARN_MANY_MAPPINGS_BUTTON"),
            context.rect(86, 106, 47, 14),
        ) + NOT_WS_TABSTOP,
        // Search
        ltext("Search", ids.id(), context.rect(139, 109, 25, 9)) + NOT_WS_GROUP,
        edittext(
            ids.named_id("ID_HEADER_SEARCH_EDIT_CONTROL"),
            context.rect(165, 106, 157, 14),
        ) + ES_AUTOHSCROLL,
        pushbutton(
            "X",
            ids.named_id("ID_CLEAR_SEARCH_BUTTON"),
            context.rect(323, 106, 11, 14),
        ) + NOT_WS_TABSTOP,
        // Source filter
        pushbutton(
            "Filter source",
            ids.named_id("ID_FILTER_BY_SOURCE_BUTTON"),
            context.rect(340, 106, 48, 14),
        ) + WS_GROUP,
        pushbutton(
            "X",
            ids.named_id("ID_CLEAR_SOURCE_FILTER_BUTTON"),
            context.rect(388, 106, 11, 14),
        ) + NOT_WS_TABSTOP,
        // Target filter
        pushbutton(
            "Filter target",
            ids.named_id("ID_FILTER_BY_TARGET_BUTTON"),
            context.rect(406, 106, 45, 14),
        ) + WS_GROUP,
        pushbutton(
            "X",
            ids.named_id("ID_CLEAR_TARGET_FILTER_BUTTON"),
            context.rect(452, 106, 11, 14),
        ) + NOT_WS_TABSTOP,
    ];
    let divider_controls = [
        divider(ids.id(), context.rect(0, 41, HEADER_PANEL_WIDTH, 1)),
        divider(ids.id(), context.rect(0, 62, HEADER_PANEL_WIDTH, 1)),
        // divider(ids.id(), context.rect(0, 123, HEADER_PANEL_WIDTH, 1)),
    ];
    Dialog {
        id: ids.named_id("ID_HEADER_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: context.rect(0, 0, HEADER_PANEL_WIDTH, HEADER_PANEL_HEIGHT),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD, WS_VISIBLE]),
        controls: upper_part_controls
            .into_iter()
            .chain(show_controls)
            .chain(lower_part_controls)
            .chain(divider_controls)
            .collect(),
        ..context.default_dialog()
    }
}
