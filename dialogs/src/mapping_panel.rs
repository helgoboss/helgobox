use crate::base::*;
use crate::ext::*;

pub fn create(context: &mut Context) -> Dialog {
    use Style::*;
    let mapping_controls = [
        groupbox("Mapping", context.id(), context.rect(7, 7, 435, 60)),
        ltext("Feedback", context.id(), context.rect(11, 53, 34, 9)) + NOT_WS_GROUP,
        combobox(
            context.named_id("ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX"),
            context.rect(48, 51, 120, 15),
        ) + CBS_DROPDOWNLIST
            + CBS_HASSTRINGS
            + WS_TABSTOP,
        checkbox(
            "Show in projection",
            context.named_id("ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX"),
            context.rect(180, 53, 74, 8),
        ) + WS_GROUP
            + WS_TABSTOP,
        pushbutton(
            "Advanced settings",
            context.named_id("ID_MAPPING_ADVANCED_BUTTON"),
            context.rect(259, 50, 87, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Find in mapping list",
            context.named_id("ID_MAPPING_FIND_IN_LIST_BUTTON"),
            context.rect(352, 50, 87, 14),
        ) + NOT_WS_TABSTOP,
    ];
    let source_controls = [
        groupbox("Source", context.id(), context.rect(7, 67, 165, 165)) + WS_GROUP,
        pushbutton(
            "Learn",
            context.named_id("ID_SOURCE_LEARN_BUTTON"),
            context.rect(11, 77, 157, 14),
        ),
        ltext("Category", context.id(), context.rect(11, 98, 29, 9)) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_SOURCE_CATEGORY_COMBO_BOX"),
            context.rect(48, 96, 120, 15),
        ) + WS_TABSTOP,
        ltext(
            "Type",
            context.named_id("ID_SOURCE_TYPE_LABEL_TEXT"),
            context.rect(11, 118, 32, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_SOURCE_TYPE_COMBO_BOX"),
            context.rect(48, 116, 120, 15),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        ltext(
            "Message",
            context.named_id("ID_SOURCE_MIDI_MESSAGE_TYPE_LABEL_TEXT"),
            context.rect(11, 138, 30, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Channel",
            context.named_id("ID_SOURCE_CHANNEL_LABEL"),
            context.rect(11, 138, 32, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_SOURCE_CHANNEL_COMBO_BOX"),
            context.rect(48, 136, 120, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        edittext(
            context.named_id("ID_SOURCE_LINE_3_EDIT_CONTROL"),
            context.rect(48, 135, 120, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            context.named_id("ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX"),
            context.rect(48, 136, 120, 15),
        ) + WS_TABSTOP,
        ltext(
            "Note/CC number",
            context.named_id("ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT"),
            context.rect(11, 158, 34, 9),
        ) + NOT_WS_GROUP,
        checkbox(
            "RPN",
            context.named_id("ID_SOURCE_RPN_CHECK_BOX"),
            context.rect(48, 158, 30, 8),
        ) + WS_TABSTOP,
        dropdown(
            context.named_id("ID_SOURCE_LINE_4_COMBO_BOX_1"),
            context.rect(47, 156, 26, 15),
        ) + WS_TABSTOP,
        edittext(
            context.named_id("ID_SOURCE_NUMBER_EDIT_CONTROL"),
            context.rect(87, 155, 80, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            context.named_id("ID_SOURCE_NUMBER_COMBO_BOX"),
            context.rect(84, 156, 84, 15),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        pushbutton(
            "Pick",
            context.named_id("ID_SOURCE_LINE_4_BUTTON"),
            context.rect(47, 155, 26, 14),
        ),
        ltext(
            "Character",
            context.named_id("ID_SOURCE_CHARACTER_LABEL_TEXT"),
            context.rect(11, 178, 32, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_SOURCE_CHARACTER_COMBO_BOX"),
            context.rect(48, 176, 120, 15),
        ) + WS_TABSTOP,
        edittext(
            context.named_id("ID_SOURCE_LINE_5_EDIT_CONTROL"),
            context.rect(48, 176, 120, 14),
        ) + ES_AUTOHSCROLL,
        checkbox(
            "14-bit values",
            context.named_id("ID_SOURCE_14_BIT_CHECK_BOX"),
            context.rect(47, 192, 56, 8),
        ) + WS_TABSTOP,
        ltext(
            "Address",
            context.named_id("ID_SOURCE_OSC_ADDRESS_LABEL_TEXT"),
            context.rect(11, 202, 139, 9),
        ) + NOT_WS_GROUP,
        edittext(
            context.named_id("ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL"),
            context.rect(11, 213, 140, 14),
        ) + ES_AUTOHSCROLL,
        pushbutton(
            "...",
            context.named_id("ID_SOURCE_SCRIPT_DETAIL_BUTTON"),
            context.rect(155, 213, 13, 14),
        ),
    ];
    let target_controls = [
        groupbox("Target", context.id(), context.rect(177, 67, 265, 165)),
        pushbutton(
            "Learn",
            context.named_id("ID_TARGET_LEARN_BUTTON"),
            context.rect(181, 77, 46, 14),
        ) + WS_GROUP,
        pushbutton(
            "Go there",
            context.named_id("ID_TARGET_OPEN_BUTTON"),
            context.rect(232, 77, 46, 14),
        ) + NOT_WS_TABSTOP,
        ltext("Hint", context.id(), context.rect(283, 80, 155, 9)) + WS_TABSTOP,
        ltext("Type", context.id(), context.rect(181, 98, 35, 9)) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_TARGET_CATEGORY_COMBO_BOX"),
            context.rect(220, 96, 58, 15),
        ) + WS_TABSTOP,
        dropdown(
            context.named_id("ID_TARGET_TYPE_COMBO_BOX"),
            context.rect(283, 96, 155, 15),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        ltext(
            "Action name",
            context.named_id("ID_TARGET_LINE_2_LABEL_2"),
            context.rect(220, 118, 189, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Hint",
            context.named_id("ID_TARGET_LINE_2_LABEL_3"),
            context.rect(412, 118, 26, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Line 2",
            context.named_id("ID_TARGET_LINE_2_LABEL_1"),
            context.rect(181, 118, 35, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_TARGET_LINE_2_COMBO_BOX_1"),
            context.rect(220, 116, 58, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        edittext(
            context.named_id("ID_TARGET_LINE_2_EDIT_CONTROL"),
            context.rect(282, 115, 127, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            context.named_id("ID_TARGET_LINE_2_COMBO_BOX_2"),
            context.rect(283, 116, 127, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        pushbutton(
            "Pick",
            context.named_id("ID_TARGET_LINE_2_BUTTON"),
            context.rect(412, 114, 26, 14),
        ),
        ltext(
            "Line 3",
            context.named_id("ID_TARGET_LINE_3_LABEL_1"),
            context.rect(181, 138, 35, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_TARGET_LINE_3_COMBO_BOX_1"),
            context.rect(220, 136, 58, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        edittext(
            context.named_id("ID_TARGET_LINE_3_EDIT_CONTROL"),
            context.rect(282, 135, 127, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            context.named_id("ID_TARGET_LINE_3_COMBO_BOX_2"),
            context.rect(283, 136, 155, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        ltext(
            "Parameter",
            context.named_id("ID_TARGET_LINE_3_LABEL_2"),
            context.rect(282, 138, 127, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Hint",
            context.named_id("ID_TARGET_LINE_3_LABEL_3"),
            context.rect(412, 138, 26, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Pick",
            context.named_id("ID_TARGET_LINE_3_BUTTON"),
            context.rect(412, 134, 26, 14),
        ),
        ltext(
            "Line 4",
            context.named_id("ID_TARGET_LINE_4_LABEL_1"),
            context.rect(181, 158, 35, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            context.named_id("ID_TARGET_LINE_4_COMBO_BOX_1"),
            context.rect(220, 156, 58, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        edittext(
            context.named_id("ID_TARGET_LINE_4_EDIT_CONTROL"),
            context.rect(282, 155, 127, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            context.named_id("ID_TARGET_LINE_4_COMBO_BOX_2"),
            context.rect(283, 156, 155, 15),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        ltext(
            "Parameter",
            context.named_id("ID_TARGET_LINE_4_LABEL_2"),
            context.rect(220, 158, 189, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Take!",
            context.named_id("ID_TARGET_LINE_4_BUTTON"),
            context.rect(412, 154, 26, 14),
        ),
        pushbutton(
            "Hint",
            context.named_id("ID_TARGET_LINE_4_LABEL_3"),
            context.rect(412, 158, 26, 9),
        ),
        ltext(
            "Line 5",
            context.named_id("ID_TARGET_LINE_5_LABEL_1"),
            context.rect(181, 178, 35, 9),
        ) + NOT_WS_GROUP,
        edittext(
            context.named_id("ID_TARGET_LINE_5_EDIT_CONTROL"),
            context.rect(282, 175, 127, 14),
        ) + ES_AUTOHSCROLL,
        checkbox(
            "Monitoring FX",
            context.named_id("ID_TARGET_CHECK_BOX_1"),
            context.rect(181, 175, 68, 8),
        ) + WS_TABSTOP,
        checkbox(
            "Track must be selected",
            context.named_id("ID_TARGET_CHECK_BOX_2"),
            context.rect(255, 175, 101, 8),
        ) + WS_TABSTOP,
        checkbox(
            "FX must have focus",
            context.named_id("ID_TARGET_CHECK_BOX_3"),
            context.rect(363, 175, 76, 8),
        ) + WS_TABSTOP,
        checkbox(
            "Monitoring FX",
            context.named_id("ID_TARGET_CHECK_BOX_4"),
            context.rect(181, 195, 69, 8),
        ) + WS_TABSTOP,
        checkbox(
            "Track must be selected",
            context.named_id("ID_TARGET_CHECK_BOX_5"),
            context.rect(255, 195, 101, 8),
        ) + WS_TABSTOP,
        checkbox(
            "FX must have focus",
            context.named_id("ID_TARGET_CHECK_BOX_6"),
            context.rect(363, 195, 76, 8),
        ) + WS_TABSTOP,
        ltext(
            "Value",
            context.named_id("ID_TARGET_VALUE_LABEL_TEXT"),
            context.rect(182, 216, 19, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Off",
            context.named_id("ID_TARGET_VALUE_OFF_BUTTON"),
            context.rect(210, 213, 32, 14),
        ),
        pushbutton(
            "On",
            context.named_id("ID_TARGET_VALUE_ON_BUTTON"),
            context.rect(250, 213, 32, 14),
        ),
        slider(
            context.named_id("ID_TARGET_VALUE_SLIDER_CONTROL"),
            context.rect(215, 213, 74, 15),
        ) + WS_TABSTOP,
        edittext(
            context.named_id("ID_TARGET_VALUE_EDIT_CONTROL"),
            context.rect(289, 213, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "%  1 ms",
            context.named_id("ID_TARGET_VALUE_TEXT"),
            context.rect(321, 216, 71, 9),
        ) + SS_WORDELLIPSIS
            + NOT_WS_GROUP,
        pushbutton(
            "bpm (bpm)",
            context.named_id("ID_TARGET_UNIT_BUTTON"),
            context.rect(396, 213, 43, 14),
        ),
    ];
    let divider_controls = [];
    Dialog {
        id: context.named_id("ID_MAPPING_PANEL"),
        caption: "Edit mapping",
        kind: DialogKind::DIALOGEX,
        rect: context.rect(0, 0, 451, 532),
        styles: Styles(vec![
            DS_SETFONT,
            DS_MODALFRAME,
            DS_3DLOOK,
            DS_CENTER,
            WS_POPUP,
            WS_VISIBLE,
            WS_CAPTION,
            WS_SYSMENU,
        ]),
        controls: mapping_controls
            .into_iter()
            .chain(source_controls.into_iter())
            .chain(target_controls.into_iter())
            .chain(divider_controls.into_iter())
            .collect(),
        ..context.default_dialog()
    }
}
