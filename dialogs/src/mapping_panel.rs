use crate::base::*;
use crate::ext::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Condition::*;
    use Style::*;
    let mapping_controls = [
        // groupbox(
        //     "Mapping",
        //     ids.named_id("ID_MAPPING_PANEL_LABEL"),
        //     context.rect(7, 1, 435, 67),
        // ),
        ltext(
            "Mapping",
            ids.named_id("ID_MAPPING_PANEL_MAPPING_LABEL"),
            context.rect(7, 1, 435, 9),
        ),
        ltext(
            "Feedback",
            ids.named_id("ID_MAPPING_PANEL_FEEDBACK_LABEL"),
            context.rect(11, 53, 34, 9),
        ) + NOT_WS_GROUP,
        combobox(
            ids.named_id("ID_MAPPING_FEEDBACK_SEND_BEHAVIOR_COMBO_BOX"),
            context.rect(48, 51, 120, 15),
        ) + CBS_DROPDOWNLIST
            + CBS_HASSTRINGS
            + WS_TABSTOP,
        context.checkbox(
            "Show in projection",
            ids.named_id("ID_MAPPING_SHOW_IN_PROJECTION_CHECK_BOX"),
            rect(180, 53, 74, 8),
        ) + WS_GROUP
            + WS_TABSTOP,
        pushbutton(
            "Advanced settings",
            ids.named_id("ID_MAPPING_ADVANCED_BUTTON"),
            context.rect(259, 50, 87, 14),
        ) + NOT_WS_TABSTOP,
        pushbutton(
            "Find in mapping list",
            ids.named_id("ID_MAPPING_FIND_IN_LIST_BUTTON"),
            context.rect(352, 50, 87, 14),
        ) + NOT_WS_TABSTOP,
    ];
    let source_controls = [
        // groupbox(
        //     "Source",
        //     ids.named_id("ID_SOURCE_PANEL_LABEL"),
        //     context.rect(7, 67, 165, 165),
        // ) + WS_GROUP,
        ltext(
            "Source",
            ids.named_id("ID_MAPPING_PANEL_SOURCE_LABEL"),
            context.rect(7, 67, 165, 9),
        ) + WS_GROUP,
        pushbutton(
            "Learn",
            ids.named_id("ID_SOURCE_LEARN_BUTTON"),
            context.rect(11, 77, 157, 14),
        ),
        ltext(
            "Category",
            ids.named_id("ID_MAPPING_PANEL_SOURCE_CATEGORY_LABEL"),
            context.rect(11, 98, 31, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_SOURCE_CATEGORY_COMBO_BOX"),
            context.rect(48, 96, 120, 15),
        ) + WS_TABSTOP,
        ltext(
            "Type",
            ids.named_id("ID_SOURCE_TYPE_LABEL_TEXT"),
            context.rect(11, 118, 32, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_SOURCE_TYPE_COMBO_BOX"),
            context.rect(48, 116, 120, 15),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        ltext(
            "Message",
            ids.named_id("ID_SOURCE_MIDI_MESSAGE_TYPE_LABEL_TEXT"),
            context.rect(11, 138, 30, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Channel",
            ids.named_id("ID_SOURCE_CHANNEL_LABEL"),
            context.rect(11, 138, 32, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_SOURCE_CHANNEL_COMBO_BOX"),
            context.rect(48, 136, 120, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SOURCE_LINE_3_EDIT_CONTROL"),
            context.rect(48, 135, 120, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            ids.named_id("ID_SOURCE_MIDI_CLOCK_TRANSPORT_MESSAGE_TYPE_COMBOX_BOX"),
            context.rect(48, 136, 120, 15),
        ) + WS_TABSTOP,
        ltext(
            "Note/CC number",
            ids.named_id("ID_SOURCE_NOTE_OR_CC_NUMBER_LABEL_TEXT"),
            context.rect(11, 158, 34, 9),
        ) + NOT_WS_GROUP,
        context.checkbox(
            "RPN",
            ids.named_id("ID_SOURCE_RPN_CHECK_BOX"),
            rect(48, 158, 30, 8),
        ) + WS_TABSTOP,
        dropdown(
            ids.named_id("ID_SOURCE_LINE_4_COMBO_BOX_1"),
            context.rect(47, 156, 26, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SOURCE_NUMBER_EDIT_CONTROL"),
            context.rect(87, 155, 80, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            ids.named_id("ID_SOURCE_NUMBER_COMBO_BOX"),
            context.rect(84, 156, 84, 15),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        pushbutton(
            "Pick",
            ids.named_id("ID_SOURCE_LINE_4_BUTTON"),
            context.rect(47, 155, 26, 14),
        ),
        ltext(
            "Character",
            ids.named_id("ID_SOURCE_CHARACTER_LABEL_TEXT"),
            context.rect(11, 178, 32, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_SOURCE_CHARACTER_COMBO_BOX"),
            context.rect(48, 176, 120, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SOURCE_LINE_5_EDIT_CONTROL"),
            context.rect(48, 176, 120, 14),
        ) + ES_AUTOHSCROLL,
        context.checkbox(
            "14-bit values",
            ids.named_id("ID_SOURCE_14_BIT_CHECK_BOX"),
            rect(47, 192, 56, 8),
        ) + WS_TABSTOP,
        ltext(
            "Address",
            ids.named_id("ID_SOURCE_OSC_ADDRESS_LABEL_TEXT"),
            context.rect(11, 202, 139, 9),
        ) + NOT_WS_GROUP,
        edittext(
            ids.named_id("ID_SOURCE_OSC_ADDRESS_PATTERN_EDIT_CONTROL"),
            context.rect(11, 213, 140, 14),
        ) + ES_AUTOHSCROLL,
        pushbutton(
            "...",
            ids.named_id("ID_SOURCE_SCRIPT_DETAIL_BUTTON"),
            context.rect(155, 213, 13, 14),
        ),
    ];
    let target_controls = [
        // groupbox(
        //     "Target",
        //     ids.named_id("ID_TARGET_PANEL_LABEL"),
        //     context.rect(177, 67, 265, 165),
        // ),
        ltext(
            "Target",
            ids.named_id("ID_MAPPING_PANEL_TARGET_LABEL"),
            context.rect(177, 67, 265, 9),
        ),
        pushbutton(
            "Learn",
            ids.named_id("ID_TARGET_LEARN_BUTTON"),
            context.rect(181, 77, 50, 14),
        ) + WS_GROUP,
        pushbutton(
            "Menu",
            ids.named_id("ID_TARGET_MENU_BUTTON"),
            context.rect(236, 77, 42, 14),
        ) + NOT_WS_TABSTOP,
        ltext(
            "Hint",
            ids.named_id("ID_TARGET_HINT"),
            context.rect(285, 80, 155, 9),
        ) + WS_TABSTOP,
        ltext(
            "Type",
            ids.named_id("ID_MAPPING_PANEL_TARGET_TYPE_LABEL"),
            context.rect(181, 98, 35, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_TARGET_CATEGORY_COMBO_BOX"),
            context.rect(220, 96, 58, 15),
        ) + WS_TABSTOP,
        pushbutton(
            "Target type",
            ids.named_id("ID_TARGET_TYPE_BUTTON"),
            context.rect(283, 96, 155, 15),
        ),
        ltext(
            "Action name",
            ids.named_id("ID_TARGET_LINE_2_LABEL_2"),
            context.rect(220, 118, 189, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Hint",
            ids.named_id("ID_TARGET_LINE_2_LABEL_3"),
            context.rect(412, 118, 26, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Line 2",
            ids.named_id("ID_TARGET_LINE_2_LABEL_1"),
            context.rect(181, 118, 35, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_TARGET_LINE_2_COMBO_BOX_1"),
            context.rect(220, 116, 58, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        edittext(
            ids.named_id("ID_TARGET_LINE_2_EDIT_CONTROL"),
            context.rect(282, 115, 127, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            ids.named_id("ID_TARGET_LINE_2_COMBO_BOX_2"),
            context.rect(283, 116, 127, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        pushbutton(
            "Pick",
            ids.named_id("ID_TARGET_LINE_2_BUTTON"),
            context.rect(412, 114, 26, 14),
        ),
        ltext(
            "Line 3",
            ids.named_id("ID_TARGET_LINE_3_LABEL_1"),
            context.rect(181, 138, 35, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_TARGET_LINE_3_COMBO_BOX_1"),
            context.rect(220, 136, 58, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        edittext(
            ids.named_id("ID_TARGET_LINE_3_EDIT_CONTROL"),
            context.rect(282, 135, 127, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            ids.named_id("ID_TARGET_LINE_3_COMBO_BOX_2"),
            context.rect(283, 136, 155, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        ltext(
            "Parameter",
            ids.named_id("ID_TARGET_LINE_3_LABEL_2"),
            context.rect(220, 138, 189, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Hint",
            ids.named_id("ID_TARGET_LINE_3_LABEL_3"),
            context.rect(412, 138, 26, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Pick",
            ids.named_id("ID_TARGET_LINE_3_BUTTON"),
            context.rect(412, 134, 26, 14),
        ),
        ltext(
            "Line 4",
            ids.named_id("ID_TARGET_LINE_4_LABEL_1"),
            context.rect(181, 158, 35, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_TARGET_LINE_4_COMBO_BOX_1"),
            context.rect(220, 156, 58, 30),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        edittext(
            ids.named_id("ID_TARGET_LINE_4_EDIT_CONTROL"),
            context.rect(282, 155, 127, 14),
        ) + ES_AUTOHSCROLL,
        dropdown(
            ids.named_id("ID_TARGET_LINE_4_COMBO_BOX_2"),
            context.rect(283, 156, 155, 15),
        ) + WS_VSCROLL
            + WS_TABSTOP,
        ltext(
            "Parameter",
            ids.named_id("ID_TARGET_LINE_4_LABEL_2"),
            context.rect(220, 158, 189, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Take!",
            ids.named_id("ID_TARGET_LINE_4_BUTTON"),
            context.rect(412, 154, 26, 14),
        ),
        ltext(
            "Hint",
            ids.named_id("ID_TARGET_LINE_4_LABEL_3"),
            context.rect(412, 158, 26, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Line 5",
            ids.named_id("ID_TARGET_LINE_5_LABEL_1"),
            context.rect(181, 178, 35, 9),
        ) + NOT_WS_GROUP,
        edittext(
            ids.named_id("ID_TARGET_LINE_5_EDIT_CONTROL"),
            context.rect(282, 175, 127, 14),
        ) + ES_AUTOHSCROLL,
        context.checkbox(
            "Monitoring FX",
            ids.named_id("ID_TARGET_CHECK_BOX_1"),
            rect(181, 175, 68, 8),
        ) + WS_TABSTOP,
        context.checkbox(
            "Track must be selected",
            ids.named_id("ID_TARGET_CHECK_BOX_2"),
            rect(255, 175, 101, 8),
        ) + WS_TABSTOP,
        context.checkbox(
            "FX must have focus",
            ids.named_id("ID_TARGET_CHECK_BOX_3"),
            rect(363, 175, 76, 8),
        ) + WS_TABSTOP,
        context.checkbox(
            "Monitoring FX",
            ids.named_id("ID_TARGET_CHECK_BOX_4"),
            rect(181, 195, 69, 8),
        ) + WS_TABSTOP,
        context.checkbox(
            "Track must be selected",
            ids.named_id("ID_TARGET_CHECK_BOX_5"),
            rect(255, 195, 101, 8),
        ) + WS_TABSTOP,
        context.checkbox(
            "FX must have focus",
            ids.named_id("ID_TARGET_CHECK_BOX_6"),
            rect(363, 195, 76, 8),
        ) + WS_TABSTOP,
        ltext(
            "Value",
            ids.named_id("ID_TARGET_VALUE_LABEL_TEXT"),
            context.rect(182, 216, 20, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            "Off",
            ids.named_id("ID_TARGET_VALUE_OFF_BUTTON"),
            context.rect(210, 213, 32, 14),
        ),
        pushbutton(
            "On",
            ids.named_id("ID_TARGET_VALUE_ON_BUTTON"),
            context.rect(250, 213, 32, 14),
        ),
        slider(
            ids.named_id("ID_TARGET_VALUE_SLIDER_CONTROL"),
            context.rect(215, 213, 74, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_TARGET_VALUE_EDIT_CONTROL"),
            context.rect(289, 213, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "%  1 ms",
            ids.named_id("ID_TARGET_VALUE_TEXT"),
            context.rect(321, 216, 71, 9),
        ) + SS_WORDELLIPSIS
            + NOT_WS_GROUP,
        pushbutton(
            "bpm (bpm)",
            ids.named_id("ID_TARGET_UNIT_BUTTON"),
            context.rect(393, 213, 40, 14),
        ),
    ];
    let glue_controls = [
        // groupbox(
        //     "Glue",
        //     ids.named_id("ID_GLUE_PANEL_LABEL"),
        //     context.rect(7, 232, 435, 239),
        // ),
        ltext(
            "Glue",
            ids.named_id("ID_MAPPING_PANEL_GLUE_LABEL"),
            context.rect(7, 232, 435, 9),
        ),
        pushbutton(
            "Reset to defaults",
            ids.named_id("ID_SETTINGS_RESET_BUTTON"),
            context.rect(11, 243, 211, 14),
        ),
        ltext(
            "Source",
            ids.named_id("ID_SETTINGS_SOURCE_LABEL"),
            context.rect(15, 281, 24, 9),
        ) + NOT_WS_GROUP,
        groupbox(
            "Source",
            ids.named_id("ID_SETTINGS_SOURCE_GROUP"),
            context.rect(55, 270, 74, 15),
        ) + WS_GROUP
            + SkipOnMacOs,
        ltext(
            "Min",
            ids.named_id("ID_SETTINGS_SOURCE_MIN_LABEL"),
            context.rect(41, 273, 15, 9),
        ) + NOT_WS_GROUP,
        slider(
            ids.named_id("ID_SETTINGS_MIN_SOURCE_VALUE_SLIDER_CONTROL"),
            context.rect(55, 270, 74, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SETTINGS_MIN_SOURCE_VALUE_EDIT_CONTROL"),
            context.rect(129, 271, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "Max",
            ids.named_id("ID_SETTINGS_SOURCE_MAX_LABEL"),
            context.rect(41, 291, 15, 9),
        ) + NOT_WS_GROUP,
        slider(
            ids.named_id("ID_SETTINGS_MAX_SOURCE_VALUE_SLIDER_CONTROL"),
            context.rect(55, 288, 74, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SETTINGS_MAX_SOURCE_VALUE_EDIT_CONTROL"),
            context.rect(129, 288, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "Out-of-range behavior",
            ids.named_id("ID_MODE_OUT_OF_RANGE_LABEL_TEXT"),
            context.rect(15, 308, 70, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_MODE_OUT_OF_RANGE_COMBOX_BOX"),
            context.rect(92, 306, 125, 15),
        ) + WS_TABSTOP,
        ltext(
            "Group interaction",
            ids.named_id("ID_MODE_GROUP_INTERACTION_LABEL_TEXT"),
            context.rect(15, 327, 71, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_MODE_GROUP_INTERACTION_COMBO_BOX"),
            context.rect(92, 325, 125, 15),
        ) + WS_TABSTOP,
        ltext(
            "Target",
            ids.named_id("ID_SETTINGS_TARGET_LABEL_TEXT"),
            context.rect(231, 281, 22, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "Value sequence",
            ids.named_id("ID_SETTINGS_TARGET_SEQUENCE_LABEL_TEXT"),
            context.rect(231, 246, 55, 9),
        ) + NOT_WS_GROUP,
        edittext(
            ids.named_id("ID_MODE_TARGET_SEQUENCE_EDIT_CONTROL"),
            context.rect(288, 243, 149, 14),
        ) + ES_AUTOHSCROLL,
        groupbox(
            "Target",
            ids.named_id("ID_SETTINGS_TARGET_GROUP"),
            context.rect(271, 270, 75, 15),
        ) + SkipOnMacOs,
        ltext(
            "Min",
            ids.named_id("ID_SETTINGS_MIN_TARGET_LABEL_TEXT"),
            context.rect(257, 273, 15, 9),
        ) + NOT_WS_GROUP,
        slider(
            ids.named_id("ID_SETTINGS_MIN_TARGET_VALUE_SLIDER_CONTROL"),
            context.rect(271, 270, 75, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SETTINGS_MIN_TARGET_VALUE_EDIT_CONTROL"),
            context.rect(347, 270, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "%  1 ms",
            ids.named_id("ID_SETTINGS_MIN_TARGET_VALUE_TEXT"),
            context.rect(379, 273, 56, 9),
        ) + SS_WORDELLIPSIS
            + NOT_WS_GROUP,
        ltext(
            "Max",
            ids.named_id("ID_SETTINGS_MAX_TARGET_LABEL_TEXT"),
            context.rect(257, 291, 15, 9),
        ) + NOT_WS_GROUP,
        slider(
            ids.named_id("ID_SETTINGS_MAX_TARGET_VALUE_SLIDER_CONTROL"),
            context.rect(271, 287, 75, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SETTINGS_MAX_TARGET_VALUE_EDIT_CONTROL"),
            context.rect(347, 288, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "%  127 ms",
            ids.named_id("ID_SETTINGS_MAX_TARGET_VALUE_TEXT"),
            context.rect(379, 291, 56, 9),
        ) + SS_WORDELLIPSIS
            + NOT_WS_GROUP,
        context.checkbox(
            "Reverse",
            ids.named_id("ID_SETTINGS_REVERSE_CHECK_BOX"),
            rect(400, 307, 39, 8),
        ) + WS_TABSTOP,
        dropdown(
            ids.named_id("IDC_MODE_FEEDBACK_TYPE_COMBO_BOX"),
            context.rect(231, 306, 163, 30),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_MODE_EEL_FEEDBACK_TRANSFORMATION_EDIT_CONTROL"),
            context.rect(231, 323, 179, 14),
        ) + ES_AUTOHSCROLL,
        pushbutton(
            "...",
            ids.named_id("IDC_MODE_FEEDBACK_TYPE_BUTTON"),
            context.rect(413, 323, 25, 14),
        ),
        groupbox(
            "For knobs/faders and buttons (control only)",
            ids.named_id("ID_MODE_KNOB_FADER_GROUP_BOX"),
            context.rect(11, 344, 211, 123),
        ),
        ltext(
            "Mode",
            ids.named_id("ID_SETTINGS_MODE_LABEL"),
            context.rect(15, 357, 20, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_SETTINGS_MODE_COMBO_BOX"),
            context.rect(50, 355, 168, 15),
        ) + WS_TABSTOP,
        ltext(
            "Takeover",
            ids.named_id("ID_MODE_TAKEOVER_LABEL"),
            context.rect(15, 409, 35, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_MODE_TAKEOVER_MODE"),
            context.rect(53, 407, 86, 15),
        ) + WS_TABSTOP,
        context.checkbox(
            "Round target value",
            ids.named_id("ID_SETTINGS_ROUND_TARGET_VALUE_CHECK_BOX"),
            rect(146, 409, 73, 8),
        ) + WS_TABSTOP,
        ltext(
            "Control transformation (EEL)",
            ids.named_id("ID_MODE_EEL_CONTROL_TRANSFORMATION_LABEL"),
            context.rect(15, 423, 95, 9),
        ) + NOT_WS_GROUP,
        edittext(
            ids.named_id("ID_MODE_EEL_CONTROL_TRANSFORMATION_EDIT_CONTROL"),
            context.rect(15, 435, 184, 14),
        ) + ES_AUTOHSCROLL,
        pushbutton(
            "...",
            ids.named_id("ID_MODE_EEL_CONTROL_TRANSFORMATION_DETAIL_BUTTON"),
            context.rect(201, 435, 13, 14),
        ),
        groupbox(
            "For encoders and incremental buttons (control only)",
            ids.named_id("ID_MODE_RELATIVE_GROUP_BOX"),
            context.rect(227, 344, 211, 61),
        ),
        ltext(
            "Step size",
            ids.named_id("ID_SETTINGS_STEP_SIZE_LABEL_TEXT"),
            context.rect(231, 366, 30, 9),
        ) + NOT_WS_GROUP,
        groupbox(
            "Step size",
            ids.named_id("ID_SETTINGS_STEP_SIZE_GROUP"),
            context.rect(279, 355, 74, 15),
        ) + SkipOnMacOs,
        ltext(
            "Min",
            ids.named_id("ID_SETTINGS_MIN_STEP_SIZE_LABEL_TEXT"),
            context.rect(266, 358, 15, 9),
        ) + NOT_WS_GROUP,
        slider(
            ids.named_id("ID_SETTINGS_MIN_STEP_SIZE_SLIDER_CONTROL"),
            context.rect(279, 355, 74, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SETTINGS_MIN_STEP_SIZE_EDIT_CONTROL"),
            context.rect(353, 355, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "%  1 ms",
            ids.named_id("ID_SETTINGS_MIN_STEP_SIZE_VALUE_TEXT"),
            context.rect(385, 358, 51, 9),
        ) + SS_WORDELLIPSIS
            + NOT_WS_GROUP,
        ltext(
            "Max",
            ids.named_id("ID_SETTINGS_MAX_STEP_SIZE_LABEL_TEXT"),
            context.rect(266, 376, 15, 9),
        ) + NOT_WS_GROUP,
        slider(
            ids.named_id("ID_SETTINGS_MAX_STEP_SIZE_SLIDER_CONTROL"),
            context.rect(279, 372, 74, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_SETTINGS_MAX_STEP_SIZE_EDIT_CONTROL"),
            context.rect(353, 372, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "%  127 ms",
            ids.named_id("ID_SETTINGS_MAX_STEP_SIZE_VALUE_TEXT"),
            context.rect(385, 375, 51, 9),
        ) + SS_WORDELLIPSIS
            + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_MODE_RELATIVE_FILTER_COMBO_BOX"),
            context.rect(231, 388, 104, 15),
        ) + WS_TABSTOP,
        context.checkbox(
            "Wrap",
            ids.named_id("ID_SETTINGS_ROTATE_CHECK_BOX"),
            rect(342, 391, 30, 8),
        ) + WS_TABSTOP,
        context.checkbox(
            "Make absolute",
            ids.named_id("ID_SETTINGS_MAKE_ABSOLUTE_CHECK_BOX"),
            rect(375, 391, 60, 8),
        ) + WS_TABSTOP,
        groupbox(
            "For buttons (control only)",
            ids.named_id("ID_MODE_BUTTON_GROUP_BOX"),
            context.rect(227, 406, 211, 61),
        ),
        dropdown(
            ids.named_id("ID_MODE_FIRE_COMBO_BOX"),
            context.rect(231, 416, 131, 15),
        ) + WS_TABSTOP,
        dropdown(
            ids.named_id("ID_MODE_BUTTON_FILTER_COMBO_BOX"),
            context.rect(367, 416, 68, 15),
        ) + WS_TABSTOP,
        ltext(
            "Min",
            ids.named_id("ID_MODE_FIRE_LINE_2_LABEL_1"),
            context.rect(231, 436, 30, 9),
        ) + NOT_WS_GROUP,
        slider(
            ids.named_id("ID_MODE_FIRE_LINE_2_SLIDER_CONTROL"),
            context.rect(265, 432, 87, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_MODE_FIRE_LINE_2_EDIT_CONTROL"),
            context.rect(353, 432, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "%  1 ms",
            ids.named_id("ID_MODE_FIRE_LINE_2_LABEL_2"),
            context.rect(385, 435, 50, 9),
        ) + SS_WORDELLIPSIS
            + NOT_WS_GROUP,
        ltext(
            "Max",
            ids.named_id("ID_MODE_FIRE_LINE_3_LABEL_1"),
            context.rect(231, 454, 31, 9),
        ) + NOT_WS_GROUP,
        slider(
            ids.named_id("ID_MODE_FIRE_LINE_3_SLIDER_CONTROL"),
            context.rect(265, 449, 87, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_MODE_FIRE_LINE_3_EDIT_CONTROL"),
            context.rect(353, 449, 30, 14),
        ) + ES_AUTOHSCROLL,
        ltext(
            "%  127 ms",
            ids.named_id("ID_MODE_FIRE_LINE_3_LABEL_2"),
            context.rect(385, 452, 50, 9),
        ) + SS_WORDELLIPSIS
            + NOT_WS_GROUP,
    ];
    let footer_controls = [
        ltext(
            "Help",
            ids.named_id("ID_MAPPING_HELP_SUBJECT_LABEL"),
            context.rect(7, 475, 183, 9),
        ) + NOT_WS_GROUP,
        ltext(
            "If source is a",
            ids.named_id("ID_MAPPING_HELP_APPLICABLE_TO_LABEL"),
            context.rect(235, 475, 43, 9),
        ) + NOT_WS_GROUP,
        dropdown(
            ids.named_id("ID_MAPPING_HELP_APPLICABLE_TO_COMBO_BOX"),
            context.rect(281, 473, 161, 15),
        ) + WS_TABSTOP,
        edittext(
            ids.named_id("ID_MAPPING_HELP_LEFT_CONTENT_LABEL"),
            context.rect(7, 488, 210, 22),
        ) + ES_MULTILINE
            + ES_READONLY
            + WS_VSCROLL,
        static_text(
            "•",
            ids.named_id("IDC_MAPPING_MATCHED_INDICATOR_TEXT"),
            context.rect(220, 495, 8, 8),
        ) + SS_LEFTNOWORDWRAP
            + WS_DISABLED
            + WS_GROUP
            + WS_TABSTOP,
        edittext(
            ids.named_id("ID_MAPPING_HELP_RIGHT_CONTENT_LABEL"),
            context.rect(7 + 225, 488, 210, 22),
        ) + ES_MULTILINE
            + ES_READONLY
            + WS_VSCROLL,
        context.checkbox(
            "Beep on success",
            ids.named_id("IDC_BEEP_ON_SUCCESS_CHECK_BOX"),
            rect(7, 516, 70, 10),
        ) + WS_TABSTOP,
        pushbutton(
            "<=",
            ids.named_id("ID_MAPPING_PANEL_PREVIOUS_BUTTON"),
            context.rect(160, 514, 30, 14),
        ),
        // We make this a normal push button instead of an OK button so that
        // pressing enter in a text field doesn't automatically trigger it.
        pushbutton(
            "OK",
            ids.named_id("ID_MAPPING_PANEL_OK"),
            context.rect(200, 514, 50, 14),
        ),
        pushbutton(
            "=>",
            ids.named_id("ID_MAPPING_PANEL_NEXT_BUTTON"),
            context.rect(260, 514, 30, 14),
        ),
        context.checkbox(
            "Enabled",
            ids.named_id("IDC_MAPPING_ENABLED_CHECK_BOX"),
            rect(405, 516, 39, 10),
        ) + WS_TABSTOP,
    ];
    Dialog {
        id: ids.named_id("ID_MAPPING_PANEL"),
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
            .chain(source_controls)
            .chain(target_controls)
            .chain(glue_controls)
            .chain(footer_controls)
            .collect(),
        ..context.default_dialog()
    }
}
