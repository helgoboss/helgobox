use crate::base::*;
use crate::ext::divider;

pub fn create(context: &mut Context) -> Dialog {
    use Style::*;
    let controls = vec![
        ctext(
            "ReaLearn",
            context.named_id("ID_MAIN_PANEL_VERSION_TEXT"),
            context.rect(66, 432, 337, 9),
        ),
        ctext(
            "Status",
            context.named_id("ID_MAIN_PANEL_STATUS_TEXT"),
            context.rect(56, 417, 356, 9),
        ) + NOT_WS_GROUP,
        pushbutton(
            " Edit tags...",
            context.named_id("IDC_EDIT_TAGS_BUTTON"),
            context.rect(416, 423, 46, 14),
        ),
        divider(context.id(), context.rect(0, 413, 470, 1)),
    ];
    Dialog {
        id: context.named_id("ID_MAIN_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: context.rect(0, 0, 470, 447),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD, WS_VISIBLE]),
        controls,
        ..context.default_dialog()
    }
}
